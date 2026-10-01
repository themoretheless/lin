//! Full-text posting lists for `search lex` / hybrid lex-half.
//!
//! Built from catalog fields with `fts: true`. Tokens are lowercase whitespace
//! words (aligned with [`crate::exec`] lex scoring). Durable stores persist
//! postings as `fts/<collection>.bin` on checkpoint (`LIN\x05`); open loads
//! them and only rebuilds missing / mismatched collections. WAL replay then
//! updates postings incrementally.

use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
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
        let mut scratch = String::new();
        let texts: Vec<&str> = fts_texts(row, &self.fields).collect();
        for text in texts {
            let lowered = lowercased(text, &mut scratch);
            for tok in lowered.split_whitespace() {
                add_posting(&mut self.postings, tok, row_idx);
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
        let kept = subtract_sorted(&base, &dels);
        let list = union_sorted(&kept, &adds);
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
        let kept = subtract_sorted(base, dels);
        Some(union_sorted(&kept, adds))
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
        {
            return;
        }
        let old = row_tokens(before, &self.fields);
        let new = row_tokens(after, &self.fields);
        for tok in &old {
            if new.contains(tok) {
                continue;
            }
            self.note_del(tok, row_idx);
        }
        for tok in &new {
            if old.contains(tok) {
                continue;
            }
            self.note_add(tok, row_idx);
        }
    }

    /// Union of posting lists for query tokens (any-token match, then residual score).
    pub fn candidate_idxs(&self, query: &str) -> Vec<usize> {
        let mut set = BTreeSet::new();
        for tok in tokenize(query) {
            if let Some(list) = self.matches(&tok) {
                set.extend(list);
            }
        }
        set.into_iter().collect()
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
fn fts_texts<'a>(row: &'a Row, fields: &'a [String]) -> impl Iterator<Item = &'a str> {
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

/// Append to a term's pending list, creating it on first use.
fn push_pending(map: &mut FxHashMap<String, Vec<usize>>, tok: &str, row_idx: usize) {
    if let Some(list) = map.get_mut(tok) {
        list.push(row_idx);
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
    let Some(k) = list.iter().position(|&p| p == row_idx) else {
        return false;
    };
    list.swap_remove(k);
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
        if list.last().copied() == Some(row_idx) {
            return;
        }
        if let Err(pos) = list.binary_search(&row_idx) {
            list.insert(pos, row_idx);
        }
    } else {
        postings.insert(tok.to_owned(), vec![row_idx]);
    }
}

fn row_tokens<'a>(row: &'a Row, fields: &[String]) -> Vec<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    let mut out = Vec::new();
    for field in fields {
        let Some(text) = row_text(row, field) else {
            continue;
        };
        if text
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_uppercase())
        {
            out.extend(text.split_whitespace().map(Cow::Borrowed));
        } else {
            out.extend(
                text.to_lowercase()
                    .split_whitespace()
                    .map(|t| Cow::Owned(t.to_owned())),
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
