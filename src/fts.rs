//! Full-text posting lists for `search lex` / hybrid lex-half.
//!
//! Built from catalog fields with `fts: true`. Tokens are lowercase whitespace
//! words (aligned with [`crate::exec`] lex scoring). Durable stores persist
//! postings as `fts/<collection>.bin` on checkpoint (`LIN\x05`); open loads
//! them and only rebuilds missing / mismatched collections. WAL replay then
//! updates postings incrementally.

use rustc_hash::FxHashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Row, row_text};

pub const FTS_DIR: &str = "fts";
pub const FTS_MAGIC: [u8; 4] = *b"LIN\x05";
const FTS_VERSION: u8 = 1;

/// Pending entries folded back into a term's base list. A removal from a sorted
/// list costs a memmove over the whole list, which for a corpus-wide token is
/// the collection's posting length; deferring it makes a single-row write cost
/// independent of how often its tokens occur.
const FOLD_AFTER: usize = 256;

/// Inverted index: token → sorted unique row indices, plus pending edits.
///
/// `postings` is the folded base; `adds` / `dels` hold changes not yet merged
/// into it. A term's live set is `(postings ∪ adds) \ dels`, which is what
/// [`Self::candidate_idxs`] reads. The three sets may overlap — an entry the
/// live set already has, or one it never had, is inert — but a position is
/// never in `adds` and `dels` at the same time.
#[derive(Debug, Clone, Default)]
pub struct FtsIndex {
    pub fields: Vec<String>,
    postings: FxHashMap<String, Vec<usize>>,
    dels: FxHashMap<String, Vec<usize>>,
    adds: FxHashMap<String, Vec<usize>>,
}

impl FtsIndex {
    pub fn empty(fields: &[String]) -> Self {
        Self {
            fields: fields.to_vec(),
            postings: FxHashMap::default(),
            dels: FxHashMap::default(),
            adds: FxHashMap::default(),
        }
    }

    pub fn build(rows: &[Row], fields: &[String]) -> Self {
        let mut idx = Self::empty(fields);
        for (i, row) in rows.iter().enumerate() {
            idx.append_row(i, row);
        }
        idx
    }

    /// Post a row whose position is new at the tail of the collection: written
    /// straight into the base list, which keeps appends allocation-cheap.
    pub fn append_row(&mut self, row_idx: usize, row: &Row) {
        // Caller should provide scratch buffer if batching; single-row path uses one-off.
        let mut scratch = String::with_capacity(128);
        self.append_row_with_scratch(row_idx, row, &mut scratch);
    }

    fn append_row_with_scratch(&mut self, row_idx: usize, row: &Row, scratch: &mut String) {
        for text in fts_texts(row, &self.fields) {
            let lowered = lowercased(text, scratch);
            for tok in lowered.split_whitespace() {
                add_posting(&mut self.postings, tok, row_idx);
            }
        }
    }

    /// Append fresh collection positions. Pending edits require the general path
    /// because a recycled tail position can still occur in an old base posting.
    pub fn append_slab(&mut self, start: usize, rows: &[Row]) {
        if self.adds.is_empty() && self.dels.is_empty() {
            let mut scratch = String::with_capacity(256); // Предварительная аллокация
            for (i, row) in rows.iter().enumerate() {
                self.append_row_with_scratch(start + i, row, &mut scratch);
            }
        } else {
            for (i, row) in rows.iter().enumerate() {
                self.insert_row(start + i, row);
            }
        }
    }

    /// Post a row at a position chosen by the writer rather than by the row
    /// count: an ordinary insert still lands as an append, while a position a
    /// base list already references (a swap-move or an undo restore) is
    /// registered through the delta set.
    pub fn insert_row(&mut self, row_idx: usize, row: &Row) {
        for tok in row_tokens(row, &self.fields) {
            self.note_add(&tok, row_idx);
        }
    }

    /// Drop one row's postings, given the row that produced them: only its own
    /// tokens are visited, so the cost is per-row rather than per-vocabulary.
    /// Callers must invoke it while `row` still sits at `row_idx`.
    pub fn remove_row(&mut self, row_idx: usize, row: &Row) {
        for tok in row_tokens(row, &self.fields) {
            self.note_del(&tok, row_idx);
        }
    }

    /// Record that `tok` matches `row_idx` from now on.
    ///
    /// A position beyond the end of the base list is an append: it goes straight
    /// onto it, which keeps the insert path free of both the memmove a sorted
    /// insert costs and the folds a stream of pending adds would trigger.
    ///
    /// Cancelling a pending removal is not the end of the edit: the removal may
    /// have been recorded against a base list that no longer holds the position
    /// (it folded away in between), so the add still has to be registered.
    fn note_add(&mut self, tok: &str, row_idx: usize) {
        if cancel(&mut self.dels, tok, row_idx) {
            push_pending(&mut self.adds, tok, row_idx);
            self.fold_if_pending(tok);
            return;
        }
        let beyond_base = !self.adds.contains_key(tok);
        match self.postings.get_mut(tok) {
            Some(list) => {
                if beyond_base && list.last().is_some_and(|&last| row_idx > last) {
                    list.push(row_idx);
                    return;
                }
                if list.binary_search(&row_idx).is_ok() {
                    return;
                }
            }
            None => {
                self.postings.insert(tok.to_owned(), vec![row_idx]);
                return;
            }
        }
        push_pending(&mut self.adds, tok, row_idx);
        self.fold_if_pending(tok);
    }

    /// Record that `tok` stops matching `row_idx`, cancelling any pending addition.
    ///
    /// The base list is deliberately not consulted: proving the position is
    /// posted means a binary search through a posting list as long as the
    /// collection, which for a corpus-wide token costs more cache misses than
    /// the whole edit. A removal of a position the base does not hold is a
    /// no-op under `(postings ∪ adds) \ dels`.
    fn note_del(&mut self, tok: &str, row_idx: usize) {
        // An addition can overlap the base after a removal was cancelled.
        // Cancelling it alone would expose the old base posting again.
        cancel(&mut self.adds, tok, row_idx);
        push_pending(&mut self.dels, tok, row_idx);
        self.fold_if_pending(tok);
    }

    fn fold_if_pending(&mut self, tok: &str) {
        let pending =
            self.dels.get(tok).map_or(0, Vec::len) + self.adds.get(tok).map_or(0, Vec::len);
        if pending > FOLD_AFTER {
            self.fold_term(tok);
        }
    }

    /// Merge one term's pending edits into its base list, restoring the sorted,
    /// unique, delta-free form. One pass over that term's postings, so a fold
    /// stays cheap relative to the `FOLD_AFTER` edits that triggered it.
    ///
    /// All three inputs are already sorted and free of duplicates, so the merge
    /// is a linear walk; sorting the concatenation made a fold the hottest part
    /// of a delete-heavy run.
    fn fold_term(&mut self, tok: &str) {
        let dels = self.dels.remove(tok).unwrap_or_default();
        let adds = self.adds.remove(tok).unwrap_or_default();
        if dels.is_empty() && adds.is_empty() {
            return;
        }
        let Some(base) = self.postings.remove(tok) else {
            if !adds.is_empty() {
                self.postings.insert(tok.to_owned(), adds);
            }
            return;
        };
        let list = merge_edits(&base, &dels, &adds);
        if !list.is_empty() {
            self.postings.insert(tok.to_owned(), list);
        }
    }

    /// Fold every term; call before serializing so the file format stays base-only.
    pub fn fold_all(&mut self) {
        let terms: Vec<String> = self.dels.keys().chain(self.adds.keys()).cloned().collect();
        for tok in terms {
            self.fold_term(&tok);
        }
    }

    /// Positions where `tok` currently matches.
    fn matches(&self, tok: &str) -> Option<Vec<usize>> {
        let dels = self.dels.get(tok).map(Vec::as_slice).unwrap_or(&[]);
        let adds = self.adds.get(tok).map(Vec::as_slice).unwrap_or(&[]);
        if dels.is_empty() && adds.is_empty() {
            return self.postings.get(tok).cloned();
        }
        let base = self.postings.get(tok).map(Vec::as_slice).unwrap_or(&[]);
        Some(merge_edits(base, dels, adds))
    }

    /// Re-point a row's postings from `from` to `to`. The token set is the row's
    /// own and does not change, so it is computed once instead of per edit —
    /// which is what a swap-move needs, since it moves one row between slots.
    pub fn move_row(&mut self, from: usize, to: usize, row: &Row) {
        for tok in row_tokens(row, &self.fields) {
            self.note_del(&tok, from);
            self.note_add(&tok, to);
        }
    }

    /// Re-point one row's postings after an in-place update: only tokens that
    /// entered or left the row touch a posting list, so the cost is per-row
    /// rather than per-vocabulary.
    pub fn sync_row(&mut self, row_idx: usize, before: &Row, after: &Row) {
        if self
            .fields
            .iter()
            .all(|field| before.get(field) == after.get(field))
            && before.get("snippet") == after.get("snippet")
        {
            return;
        }
        let old = row_tokens(before, &self.fields);
        let new = row_tokens(after, &self.fields);
        let (mut old_pos, mut new_pos) = (0, 0);
        while old_pos < old.len() && new_pos < new.len() {
            match old[old_pos].cmp(&new[new_pos]) {
                std::cmp::Ordering::Less => {
                    self.note_del(&old[old_pos], row_idx);
                    old_pos += 1;
                }
                std::cmp::Ordering::Greater => {
                    self.note_add(&new[new_pos], row_idx);
                    new_pos += 1;
                }
                std::cmp::Ordering::Equal => {
                    old_pos += 1;
                    new_pos += 1;
                }
            }
        }
        for tok in &old[old_pos..] {
            self.note_del(tok, row_idx);
        }
        for tok in &new[new_pos..] {
            self.note_add(tok, row_idx);
        }
    }

    /// Union of posting lists for query tokens (any-token match, then residual score).
    pub fn candidate_idxs(&self, query: &str) -> Vec<usize> {
        let mut candidates = Vec::new();
        for tok in tokenize(query) {
            if let Some(list) = self.matches(&tok) {
                candidates = if candidates.is_empty() {
                    list
                } else {
                    union_sorted(&candidates, &list)
                };
            }
        }
        candidates
    }

    pub fn encode(&self, generation: u64, nrows: u64) -> Vec<u8> {
        let mut terms: Vec<(&String, &Vec<usize>)> = self.postings.iter().collect();
        terms.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = Vec::with_capacity(64 + terms.len() * 16);
        out.extend_from_slice(&FTS_MAGIC);
        out.push(FTS_VERSION);
        out.extend_from_slice(&generation.to_le_bytes());
        out.extend_from_slice(&nrows.to_le_bytes());
        out.extend_from_slice(&(self.fields.len() as u32).to_le_bytes());
        for f in &self.fields {
            let b = f.as_bytes();
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
        }
        out.extend_from_slice(&(terms.len() as u32).to_le_bytes());
        for (tok, list) in terms {
            let b = tok.as_bytes();
            out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for idx in list {
                out.extend_from_slice(&(*idx as u32).to_le_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<(u64, u64, Self), Error> {
        if bytes.len() < 4 + 1 + 8 + 8 + 4 || bytes[..4] != FTS_MAGIC {
            return Err(io_err("bad fts magic"));
        }
        if bytes[4] != FTS_VERSION {
            return Err(io_err(format!("unsupported fts version {}", bytes[4])));
        }
        let mut i = 5;
        let generation = read_u64(bytes, &mut i)?;
        let nrows = read_u64(bytes, &mut i)?;
        let nfields = read_u32(bytes, &mut i)? as usize;
        let mut fields = Vec::with_capacity(nfields);
        for _ in 0..nfields {
            fields.push(read_str(bytes, &mut i)?);
        }
        let nterms = read_u32(bytes, &mut i)? as usize;
        let mut postings = FxHashMap::with_capacity_and_hasher(nterms, Default::default());
        for _ in 0..nterms {
            let tok = read_str(bytes, &mut i)?;
            let n = read_u32(bytes, &mut i)? as usize;
            let mut list = Vec::with_capacity(n);
            for _ in 0..n {
                list.push(read_u32(bytes, &mut i)? as usize);
            }
            postings.insert(tok, list);
        }
        Ok((
            generation,
            nrows,
            Self {
                fields,
                postings,
                dels: FxHashMap::default(),
                adds: FxHashMap::default(),
            },
        ))
    }
}

fn read_u32(b: &[u8], i: &mut usize) -> Result<u32, Error> {
    let end = i.saturating_add(4);
    if end > b.len() {
        return Err(io_err("truncated fts"));
    }
    let v = u32::from_le_bytes(b[*i..end].try_into().unwrap());
    *i = end;
    Ok(v)
}

fn read_u64(b: &[u8], i: &mut usize) -> Result<u64, Error> {
    let end = i.saturating_add(8);
    if end > b.len() {
        return Err(io_err("truncated fts"));
    }
    let v = u64::from_le_bytes(b[*i..end].try_into().unwrap());
    *i = end;
    Ok(v)
}

fn read_str(b: &[u8], i: &mut usize) -> Result<String, Error> {
    let n = read_u32(b, i)? as usize;
    let end = i.saturating_add(n);
    if end > b.len() {
        return Err(io_err("truncated fts token"));
    }
    let s = std::str::from_utf8(&b[*i..end]).map_err(io_err)?;
    *i = end;
    Ok(s.to_string())
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Text of every field an index tokenizes: the `fts` fields, plus `snippet`
/// when it is not one of them (lex scoring reads it).
fn fts_texts<'a: 'b, 'b>(row: &'a Row, fields: &'b [String]) -> impl Iterator<Item = &'a str> + 'b {
    let snippet = if fields.iter().any(|f| f == "snippet") {
        None
    } else {
        row_text(row, "snippet")
    };
    fields
        .iter()
        .filter_map(move |f| row_text(row, f))
        .chain(snippet)
}

/// `text` lowercased, borrowed from `text` itself when lowering is a no-op.
pub(crate) fn lowercased<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    if text.is_ascii() {
        if !text.bytes().any(|b| b.is_ascii_uppercase()) {
            return text;
        }
        scratch.clear();
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    scratch.clear();
    for c in text.chars() {
        scratch.extend(c.to_lowercase());
    }
    scratch
}

/// `a \ b` for two sorted, duplicate-free lists: one forward walk with a cursor,
/// which is what posting folds and pending-aware reads are shaped like.
#[cfg(test)]
fn subtract_sorted(a: &[usize], b: &[usize]) -> Vec<usize> {
    if b.is_empty() {
        return a.to_vec();
    }
    let mut out = Vec::with_capacity(a.len());
    let mut j = 0;
    for &x in a {
        while j < b.len() && b[j] < x {
            j += 1;
        }
        if j < b.len() && b[j] == x {
            continue;
        }
        out.push(x);
    }
    out
}

/// `a ∪ b` for two sorted lists, collapsing any overlap between them.
fn union_sorted(a: &[usize], b: &[usize]) -> Vec<usize> {
    if b.is_empty() {
        return a.to_vec();
    }
    if a.is_empty() {
        return b.to_vec();
    }
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let v = if a[i] <= b[j] {
            i += 1;
            a[i - 1]
        } else {
            j += 1;
            b[j - 1]
        };
        if out.last().copied() != Some(v) {
            out.push(v);
        }
    }
    for &v in &a[i..] {
        if out.last().copied() != Some(v) {
            out.push(v);
        }
    }
    for &v in &b[j..] {
        if out.last().copied() != Some(v) {
            out.push(v);
        }
    }
    out
}

/// Insert into a sorted, unique pending list; merges rely on this invariant.
fn push_pending(map: &mut FxHashMap<String, Vec<usize>>, tok: &str, row_idx: usize) {
    if let Some(list) = map.get_mut(tok) {
        if let Err(pos) = list.binary_search(&row_idx) {
            list.insert(pos, row_idx);
        }
    } else {
        map.insert(tok.to_owned(), vec![row_idx]);
    }
}

/// Drop `row_idx` from a term's pending list, reporting whether it was there.
/// A cancelled entry means the base list already holds the right answer.
fn cancel(map: &mut FxHashMap<String, Vec<usize>>, tok: &str, row_idx: usize) -> bool {
    let Some(list) = map.get_mut(tok) else {
        return false;
    };
    let Ok(k) = list.binary_search(&row_idx) else {
        return false;
    };
    list.remove(k);
    let drained = list.is_empty();
    if drained {
        map.remove(tok);
    }
    true
}

/// Add `row_idx` to a token's sorted list, allocating a posting list only when
/// the token is new.
fn add_posting(postings: &mut FxHashMap<String, Vec<usize>>, tok: &str, row_idx: usize) {
    if let Some(list) = postings.get_mut(tok) {
        if let Some(&last) = list.last() {
            if last == row_idx {
                return;
            }
            if last < row_idx {
                list.push(row_idx);
                return;
            }
        }
        if let Err(pos) = list.binary_search(&row_idx) {
            list.insert(pos, row_idx);
        }
    } else {
        postings.insert(tok.to_owned(), vec![row_idx]);
    }
}

fn row_tokens<'a>(row: &'a Row, fields: &[String]) -> Vec<std::borrow::Cow<'a, str>> {
    let mut out = Vec::with_capacity(fields.len() * 10);
    for text in fts_texts(row, fields) {
        if text
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_uppercase())
        {
            out.extend(text.split_whitespace().map(std::borrow::Cow::Borrowed));
        } else {
            let lowercased = text.to_lowercase();
            out.extend(
                lowercased
                    .split_whitespace()
                    .map(|t| std::borrow::Cow::Owned(t.into())),
            );
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn fts_dir(data: &Path) -> PathBuf {
    data.join(FTS_DIR)
}

pub fn fts_path(data: &Path, name: &str) -> PathBuf {
    fts_dir(data).join(format!("{name}.bin"))
}

/// Persist a collection's postings. Pending edits are folded first, so the file
/// stays a plain base list and the on-disk format is unchanged.
pub fn write_fts(
    data: &Path,
    name: &str,
    idx: &mut FtsIndex,
    generation: u64,
    nrows: u64,
) -> Result<(), Error> {
    let dir = fts_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = fts_path(data, name);
    let tmp = path.with_extension("bin.tmp");
    idx.fold_all();
    let body = idx.encode(generation, nrows);
    crate::persist::write_through_tmp(&path, &tmp, &body)
}

pub fn read_fts(data: &Path, name: &str) -> Result<(u64, u64, FtsIndex), Error> {
    let path = fts_path(data, name);
    let mut f = File::open(&path).map_err(io_err)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(io_err)?;
    FtsIndex::decode(&buf)
}

/// Drop stale fts files not listed in `keep`.
pub fn prune_fts(data: &Path, keep: &[String]) -> Result<(), Error> {
    crate::persist::prune_bins(&fts_dir(data), keep)
}

pub fn fts_fields(catalog: &crate::catalog::Catalog, collection: &str) -> Vec<String> {
    catalog
        .collections
        .get(collection)
        .map(|c| {
            c.fields
                .iter()
                .filter(|(_, f)| f.fts)
                .map(|(n, _)| n.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Cell;

    #[test]
    fn snippet_mutations_match_rebuilt_index() {
        for fields in [vec!["title".into()], vec!["title".into(), "snippet".into()]] {
            let before = Row::from([
                ("title".into(), Cell::text_arc("shared")),
                ("snippet".into(), Cell::text_arc("OLD shared ЁЖ")),
            ]);
            let after = Row::from([
                ("title".into(), Cell::text_arc("shared")),
                ("snippet".into(), Cell::text_arc("NEW shared ИГЛА")),
            ]);
            let mut index = FtsIndex::build(&[before.clone()], &fields);
            index.sync_row(0, &before, &after);
            let reference = FtsIndex::build(&[after.clone()], &fields);
            for query in ["old", "new", "shared", "ёж", "игла"] {
                assert_eq!(
                    index.candidate_idxs(query),
                    reference.candidate_idxs(query),
                    "{query}"
                );
            }
            index.move_row(0, 1, &after);
            assert_eq!(index.candidate_idxs("new"), vec![1]);
            index.remove_row(1, &after);
            assert!(index.candidate_idxs("new shared игла").is_empty());
            index.insert_row(0, &before);
            assert_eq!(index.candidate_idxs("old ёж"), vec![0]);
            index.fold_all();
            assert_eq!(index.postings, FtsIndex::build(&[before], &fields).postings);
        }
    }

    #[test]
    fn sync_token_sets_match_rebuild() {
        let fields = vec!["title".into(), "body".into()];
        for old_mask in 0..32 {
            for new_mask in 0..32 {
                let row = |mask: usize| {
                    Row::from([(
                        "title".into(),
                        Cell::text_arc(
                            (0..5)
                                .filter(|i| mask & (1 << i) != 0)
                                .map(|i| format!("WORD{i} word{i}"))
                                .collect::<Vec<_>>()
                                .join(" "),
                        ),
                    )])
                };
                let before = row(old_mask);
                let after = row(new_mask);
                let mut index = FtsIndex::build(&[before.clone()], &fields);
                index.sync_row(0, &before, &after);
                index.fold_all();
                assert_eq!(index.postings, FtsIndex::build(&[after], &fields).postings);
            }
        }
    }

    #[test]
    #[ignore = "manual paired performance measurement"]
    fn optimization_measurement() {
        use std::{hint::black_box, time::Instant};
        let fields = vec!["title".into()];
        for text in [
            "already lowercase words",
            "UPPERCASE WORDS",
            "РУССКИЙ ТЕКСТ ЁЖ",
        ] {
            let rows = (0..10000)
                .map(|_| Row::from([("title".into(), Cell::text_arc(text))]))
                .collect::<Vec<_>>();
            let start = Instant::now();
            for _ in 0..30 {
                black_box(FtsIndex::build(black_box(&rows), &fields));
            }
            println!("build {text}: {} ns", start.elapsed().as_nanos() / 30);
        }
        for n in [10, 100, 1000] {
            let make = |shift| {
                Row::from([(
                    "title".into(),
                    Cell::text_arc(
                        (shift..n + shift)
                            .map(|i| format!("word{i}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                )])
            };
            let before = make(0);
            let after = make(n / 2);
            let mut index = FtsIndex::build(&[before.clone()], &fields);
            let start = Instant::now();
            for _ in 0..100 {
                index.sync_row(0, &before, &after);
                index.sync_row(0, &after, &before);
            }
            black_box(&index);
            println!("sync {n}: {} ns", start.elapsed().as_nanos() / 200);
            assert_eq!(index.candidate_idxs("word0"), vec![0]);
        }
    }

    #[test]
    fn slab_normalization_scratch_matches_individual_rows() {
        let fields = vec!["title".into(), "body".into()];
        let rows = ["LONG UPPERCASE TITLE", "x", "ЁЖ İ 🦔", "", "MiXeD", "lower"]
            .into_iter()
            .map(|title| {
                Row::from([
                    ("title".into(), Cell::text_arc(title)),
                    ("body".into(), Cell::text_arc("Body BODY")),
                    ("snippet".into(), Cell::text_arc("SNIPPET")),
                ])
            })
            .collect::<Vec<_>>();
        let mut slab = FtsIndex::empty(&fields);
        let mut single = FtsIndex::empty(&fields);
        slab.append_slab(17, &rows);
        for (i, row) in rows.iter().enumerate() {
            single.append_row(17 + i, row);
        }
        assert_eq!(slab.postings, single.postings);
        assert_eq!(slab.adds, single.adds);
        assert_eq!(slab.dels, single.dels);
    }

    #[test]
    fn appended_slabs_match_general_insert_after_tail_recycling() {
        let fields = vec!["title".into(), "body".into()];
        let rows = (0..300)
            .map(|i| {
                Row::from([
                    (
                        "title".into(),
                        Cell::text_arc(if i % 2 == 0 { "wal WAL ёж" } else { "other" }),
                    ),
                    ("body".into(), Cell::text_arc("wal ЁЖ ёж")),
                ])
            })
            .collect::<Vec<_>>();
        let mut fast = FtsIndex::empty(&fields);
        let mut reference = FtsIndex::empty(&fields);
        fast.append_slab(0, &rows);
        for (i, row) in rows.iter().enumerate() {
            reference.insert_row(i, row);
        }
        fast.remove_row(299, &rows[299]);
        reference.remove_row(299, &rows[299]);
        let replacement = Row::from([("title".into(), Cell::text_arc("replacement"))]);
        fast.append_slab(299, std::slice::from_ref(&replacement));
        reference.insert_row(299, &replacement);
        fast.append_slab(300, &rows);
        for (i, row) in rows.iter().enumerate() {
            reference.insert_row(300 + i, row);
        }
        for query in ["wal", "ёж", "other", "replacement", "WAL ЁЖ"] {
            assert_eq!(
                fast.candidate_idxs(query),
                reference.candidate_idxs(query),
                "{query}"
            );
        }
        fast.fold_all();
        reference.fold_all();
        assert_eq!(fast.postings, reference.postings);
    }

    #[test]
    fn repeated_term_removal_and_reinsertion_matches_current_rows() {
        let fields = vec!["title".to_owned()];
        let row = Row::from([("title".into(), Cell::text_arc("foo"))]);
        let mut index = FtsIndex::build(&[row.clone()], &fields);
        for i in 0..600 {
            index.remove_row(0, &row);
            assert!(index.candidate_idxs("foo").is_empty(), "removed at {i}");
            if i % 7 == 0 {
                index.fold_all();
            }
            index.insert_row(0, &row);
            assert_eq!(index.candidate_idxs("foo"), vec![0], "inserted at {i}");
        }
    }

    #[test]
    fn multi_term_candidates_match_scan_with_pending_edits() {
        let fields = vec!["title".to_owned()];
        let mut rows = (0..40)
            .map(|i| {
                Row::from([(
                    "title".into(),
                    Cell::text_arc(if i % 2 == 0 { "WAL ёж" } else { "wal other" }),
                )])
            })
            .collect::<Vec<_>>();
        let mut index = FtsIndex::build(&rows, &fields);
        for i in (0..40).step_by(3) {
            index.remove_row(i, &rows[i]);
            rows[i] = Row::from([("title".into(), Cell::text_arc("new ЁЖ"))]);
            index.insert_row(i, &rows[i]);
        }
        for query in [
            "",
            "missing",
            "wal",
            "WAL wal",
            "wal ёж",
            "new other missing",
            "ЁЖ new wal",
        ] {
            let terms = query.to_lowercase();
            let expected = rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| {
                    let text = row_text(row, "title").unwrap().to_lowercase();
                    terms
                        .split_whitespace()
                        .any(|term| text.split_whitespace().any(|word| word == term))
                        .then_some(i)
                })
                .collect::<Vec<_>>();
            assert_eq!(index.candidate_idxs(query), expected, "{query}");
        }
    }

    #[test]
    fn borrowed_tokens_deduplicate_and_normalize_across_fields_and_edits() {
        let row = Row::from([
            ("title".into(), Cell::text_arc("wal wal ёж WAL")),
            ("body".into(), Cell::text_arc("WAL ЁЖ ёж")),
        ]);
        let fields = vec!["title".to_owned(), "body".to_owned()];
        assert_eq!(
            row_tokens(&row, &fields)
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>(),
            vec!["wal", "ёж"]
        );
        let mut index = FtsIndex::empty(&fields);
        for i in 0..300 {
            index.insert_row(i, &row);
        }
        for i in 0..300 {
            index.remove_row(i, &row);
        }
        assert!(index.candidate_idxs("wal ёж").is_empty());
        index.insert_row(0, &row);
        index.move_row(0, 1, &row);
        assert_eq!(index.candidate_idxs("WAL ЁЖ"), vec![1]);
        index.remove_row(1, &row);
        assert!(index.candidate_idxs("wal ёж").is_empty());
    }

    #[test]
    fn posting_codec_roundtrips_and_rejects_truncation() {
        let mut row = Row::new();
        row.insert("title".into(), Cell::text_arc("durable wal"));
        row.insert("body".into(), Cell::text_arc("checkpoint postings"));
        let fields = vec![String::from("title"), String::from("body")];
        let index = FtsIndex::build(&[row], &fields);
        let encoded = index.encode(7, 1);
        let (generation, rows, decoded) = FtsIndex::decode(&encoded).unwrap();
        assert_eq!((generation, rows), (7, 1));
        assert_eq!(decoded.fields, fields);
        assert_eq!(decoded.candidate_idxs("wal"), vec![0]);
        assert!(FtsIndex::decode(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn prune_drops_stale_blobs_and_half_written_temps() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lin-prune-fts-{nanos:020}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(fts_dir(&dir)).unwrap();
        let touch = |name: &str| {
            fs::write(fts_dir(&dir).join(name), b"x").unwrap();
        };
        touch("docs.bin");
        touch("gone.bin");
        touch("docs.bin.tmp");
        touch("gone.bin.tmp");
        touch("notes.txt");

        prune_fts(&dir, &[String::from("docs")]).unwrap();

        let keep = |name: &str| fts_dir(&dir).join(name).exists();
        assert!(keep("docs.bin"), "current blob survives");
        assert!(!keep("gone.bin"), "dropped collection is pruned");
        assert!(
            !keep("docs.bin.tmp"),
            "temps never survive: prune only runs under the exclusive fence"
        );
        assert!(!keep("gone.bin.tmp"));
        assert!(keep("notes.txt"), "unrelated files left alone");
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod pending_order_tests {
    use super::*;
    #[test]
    fn ascii_lowering_and_unicode_fallback_match_original_character_mapping() {
        let ascii = (0..128u8).map(char::from).collect::<String>();
        let cases = [
            ascii.as_str(),
            "",
            "already lower 123",
            "UPPER Mixed CASE",
            "Ёж ΣΟΣ İ ß",
            "WAL ЁЖ",
            "a\0Z\t",
            "é",
        ];
        let mut scratch = String::from("previous buffer contents");
        for _ in 0..3 {
            for text in cases {
                let reference = text
                    .chars()
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                assert_eq!(lowercased(text, &mut scratch), reference);
            }
        }
        let lower = "already lower 123";
        assert_eq!(lowercased(lower, &mut scratch).as_ptr(), lower.as_ptr());
    }

    #[test]
    fn fused_merge_matches_two_pass_on_all_small_sets() {
        let sets = (0..32)
            .map(|mask| {
                (0..5)
                    .filter(|i| mask & (1 << i) != 0)
                    .collect::<Vec<usize>>()
            })
            .collect::<Vec<_>>();
        for base in &sets {
            for dels in &sets {
                for adds in &sets {
                    assert_eq!(
                        merge_edits(base, dels, adds),
                        union_sorted(&subtract_sorted(base, dels), adds)
                    );
                }
            }
        }
    }

    #[test]
    fn descending_and_random_edits_survive_folding_and_codec() {
        let mut index = FtsIndex::empty(&["body".into()]);
        index.postings.insert("wal".into(), (0..600).collect());
        let mut expected = (0..600).collect::<std::collections::BTreeSet<usize>>();
        for pos in (0..600).rev() {
            index.note_del("wal", pos);
            expected.remove(&pos);
            assert_eq!(
                index.matches("wal").unwrap_or_default(),
                expected.iter().copied().collect::<Vec<_>>()
            );
        }
        let mut state = 7u64;
        for step in 0..3000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let pos = ((state >> 17) % 900) as usize;
            if state >> 63 != 0 {
                index.note_add("wal", pos);
                expected.insert(pos);
            } else {
                index.note_del("wal", pos);
                expected.remove(&pos);
            }
            if step % 37 == 0 {
                index.fold_all();
            }
            assert_eq!(
                index.matches("wal").unwrap_or_default(),
                expected.iter().copied().collect::<Vec<_>>()
            );
        }
        index.fold_all();
        let (_, _, decoded) = FtsIndex::decode(&index.encode(2, 900)).unwrap();
        assert_eq!(
            decoded.matches("wal").unwrap_or_default(),
            expected.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn out_of_order_pending_edits_match_live_positions() {
        let mut index = FtsIndex::empty(&["body".into()]);
        index.postings.insert("wal".into(), (0..10).collect());
        let mut expected = (0..10).collect::<std::collections::BTreeSet<usize>>();
        for (add, pos) in [
            (false, 8),
            (false, 2),
            (false, 6),
            (true, 2),
            (true, 8),
            (false, 4),
            (false, 1),
            (true, 6),
            (false, 8),
        ] {
            if add {
                index.note_add("wal", pos);
                expected.insert(pos);
            } else {
                index.note_del("wal", pos);
                expected.remove(&pos);
            }
            assert_eq!(
                index.matches("wal").unwrap_or_default(),
                expected.iter().copied().collect::<Vec<_>>()
            );
        }
        index.fold_all();
        assert_eq!(
            index.matches("wal").unwrap_or_default(),
            expected.into_iter().collect::<Vec<_>>()
        );
    }
}

fn merge_edits(base: &[usize], dels: &[usize], adds: &[usize]) -> Vec<usize> {
    let mut out = Vec::with_capacity(base.len() + adds.len());
    let (mut i, mut j, mut k) = (0, 0, 0);
    loop {
        while i < base.len() {
            while j < dels.len() && dels[j] < base[i] {
                j += 1;
            }
            if j < dels.len() && dels[j] == base[i] {
                i += 1;
            } else {
                break;
            }
        }
        match (base.get(i), adds.get(k)) {
            (Some(&a), Some(&b)) if a < b => {
                out.push(a);
                i += 1;
            }
            (Some(&a), Some(&b)) if a == b => {
                out.push(a);
                i += 1;
                k += 1;
            }
            (Some(_), Some(&b)) => {
                out.push(b);
                k += 1;
            }
            (Some(&a), None) => {
                out.push(a);
                i += 1;
            }
            (None, Some(&b)) => {
                out.push(b);
                k += 1;
            }
            (None, None) => break,
        }
    }
    out
}
