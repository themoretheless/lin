//! Full-text posting lists for `search lex` / hybrid lex-half.
//!
//! Built from catalog fields with `fts: true`. Tokens are lowercase whitespace
//! words (aligned with [`crate::exec`] lex scoring). Durable stores persist
//! postings as `fts/<collection>.bin` on checkpoint (`LIN\x05`); open loads
//! them and only rebuilds missing / mismatched collections. WAL replay then
//! updates postings incrementally.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Error;
use crate::persist::io_err;
use crate::store::row::OwnedFieldSlot;
use crate::store::{Cell, Row};

pub const FTS_DIR: &str = "fts";
pub const FTS_MAGIC: [u8; 4] = *b"LIN\x05";
/// Tokenizer version stamp (`2`: contextual `str::to_lowercase` + combining mark preservation).
pub const TOKENIZER_VERSION: u32 = 2;
/// 2: tokens lowercased with `str::to_lowercase` (contextual: a word-final capital sigma
/// becomes `ς`), matching the query side; version-1 files are rebuilt.
const FTS_VERSION: u8 = 2;
const FTS_CRC_MAGIC: [u8; 4] = *b"LCRC";

/// Configurable lexical tokenizer options for [`inspect_tokens`] (`LIN-0897`..`LIN-0904`, `LIN-0912`).
#[derive(Debug, Clone)]
pub struct TokenizerConfig {
    pub min_token_bytes: usize,
    pub max_token_bytes: usize,
    pub max_tokens_per_doc: usize,
    pub stopwords: Option<FxHashSet<String>>,
    pub ascii_stem_s_ing_ed: bool,
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self {
            min_token_bytes: 1,
            max_token_bytes: 256,
            max_tokens_per_doc: 65_536,
            stopwords: None,
            ascii_stem_s_ing_ed: false,
        }
    }
}

/// Emitted token with UTF-8 byte offsets `[byte_start, byte_end)` and 0-based stream position (`LIN-0909`, `LIN-0917`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenSpan {
    pub token: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub position: usize,
}

/// Vocabulary size, posting cardinality, and pending delta-compaction debt (`LIN-0918`, `LIN-0989`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct FtsVocabularyStats {
    pub unique_terms: usize,
    pub vocabulary_bytes: usize,
    pub total_base_postings: usize,
    pub pending_add_terms: usize,
    pub pending_del_terms: usize,
    pub compaction_debt_entries: usize,
    pub max_posting_len: usize,
}

/// Structural invariant verification report for an [`FtsIndex`] (`LIN-0990`, `LIN-0991`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct FtsInvariantReport {
    pub valid: bool,
    pub unsorted_base_terms: usize,
    pub overlap_add_del_terms: usize,
    pub out_of_bounds_postings: usize,
}

/// Exact posting-by-posting comparison between a live [`FtsIndex`] and a fresh rebuild (`LIN-0992`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct FtsRebuildDiff {
    pub identical: bool,
    pub only_in_live: Vec<(String, Vec<usize>)>,
    pub only_in_rebuilt: Vec<(String, Vec<usize>)>,
}

/// Pending entries folded back into a term's base list. A removal from a sorted
/// list costs a memmove over the whole list, which for a corpus-wide token is
/// the collection's posting length; deferring it makes a single-row write cost
/// independent of how often its tokens occur.
const FOLD_AFTER: usize = 256;

fn build_fts_slots(fields: &[String]) -> SmallVec<[OwnedFieldSlot; 4]> {
    let has_snippet = fields.iter().any(|f| f == "snippet");
    let mut slots: SmallVec<[OwnedFieldSlot; 4]> = fields
        .iter()
        .map(|f| OwnedFieldSlot::new(f.as_str()))
        .collect();
    if !has_snippet {
        slots.push(OwnedFieldSlot::new("snippet"));
    }
    slots
}

#[derive(Debug, Clone, Default)]
struct FtsInner {
    postings: FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
    dels: FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
    adds: FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
}

/// Inverted index: token → sorted unique row indices, plus pending edits.
///
/// `postings` is the folded base; `adds` / `dels` hold changes not yet merged
/// into it. A term's live set is `(postings ∪ adds) \ dels`, which is what
/// [`Self::candidate_idxs`] reads. The three sets may overlap — an entry the
/// live set already has, or one it never had, is inert — but a position is
/// never in `adds` and `dels` at the same time.
///
/// The three posting maps live behind `Arc<FtsInner>` so `FtsIndex::clone()`
/// in `Store::clone_mem()` is `O(1)` copy-on-write (`Arc::clone`).
#[derive(Debug, Clone)]
pub struct FtsIndex {
    pub fields: Vec<String>,
    slots: SmallVec<[OwnedFieldSlot; 4]>,
    inner: Arc<FtsInner>,
}

impl Default for FtsIndex {
    fn default() -> Self {
        Self::empty(&[])
    }
}

impl FtsInner {
    fn append_row_with_scratch(
        &mut self,
        slots: &mut [OwnedFieldSlot],
        row_idx: usize,
        row: &Row,
        scratch: &mut String,
    ) {
        for slot in slots {
            if let Some(text) = slot.get(row).and_then(Cell::text) {
                let lowered = lowercased(text, scratch);
                for_each_word(lowered, |tok| {
                    add_posting(&mut self.postings, tok, row_idx);
                });
            }
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
        if !self.dels.is_empty() && cancel(&mut self.dels, tok, row_idx) {
            let len = push_pending(&mut self.adds, &self.postings, tok, row_idx);
            if len > FOLD_AFTER / 2 {
                self.fold_if_pending(tok);
            }
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
                self.postings.insert(Arc::from(tok), smallvec![row_idx]);
                return;
            }
        }
        let len = push_pending(&mut self.adds, &self.postings, tok, row_idx);
        if len > FOLD_AFTER / 2 {
            self.fold_if_pending(tok);
        }
    }

    /// Record that `tok` stops matching `row_idx`, cancelling any pending addition.
    ///
    /// The base list is deliberately not consulted: proving the position is
    /// posted means a binary search through a posting list as long as the
    /// collection, which for a corpus-wide token costs more cache misses than
    /// the whole edit. A removal of a position the base does not hold is a
    /// no-op under `(postings ∪ adds) \ dels`.
    fn note_del(&mut self, tok: &str, row_idx: usize) {
        if (self.adds.is_empty() || !self.adds.contains_key(tok))
            && (self.dels.is_empty() || !self.dels.contains_key(tok))
            && let Some(list) = self.postings.get_mut(tok)
        {
            if list.last() == Some(&row_idx) {
                list.pop();
                if list.is_empty() {
                    self.postings.remove(tok);
                }
                return;
            }
            if list.len() <= 32 {
                if let Ok(pos) = list.binary_search(&row_idx) {
                    list.remove(pos);
                    if list.is_empty() {
                        self.postings.remove(tok);
                    }
                }
                return;
            }
        }
        // An addition can overlap the base after a removal was cancelled.
        // Cancelling it alone would expose the old base posting again.
        if !self.adds.is_empty() {
            cancel(&mut self.adds, tok, row_idx);
        }
        let len = push_pending(&mut self.dels, &self.postings, tok, row_idx);
        if len > FOLD_AFTER / 2 {
            self.fold_if_pending(tok);
        }
    }

    #[inline]
    fn note_del_tail(&mut self, tok: &str, last: usize) {
        if (self.adds.is_empty() || !self.adds.contains_key(tok))
            && let Some(list) = self.postings.get_mut(tok)
            && list.last() == Some(&last)
        {
            list.pop();
            if list.is_empty() && (self.dels.is_empty() || !self.dels.contains_key(tok)) {
                self.postings.remove(tok);
            }
            return;
        }
        self.note_del(tok, last);
    }

    #[inline]
    fn note_move_from_tail(&mut self, tok: &str, p: usize, last: usize) {
        if (self.adds.is_empty() || !self.adds.contains_key(tok))
            && (self.dels.is_empty() || !self.dels.contains_key(tok))
            && let Some(list) = self.postings.get_mut(tok)
            && list.last() == Some(&last)
        {
            list.pop();
            if list.len() <= 32 {
                if let Err(pos) = list.binary_search(&p) {
                    list.insert(pos, p);
                }
                return;
            }
            if list.binary_search(&p).is_ok() {
                return;
            }
            let len = push_pending(&mut self.adds, &self.postings, tok, p);
            if len > FOLD_AFTER / 2 {
                self.fold_if_pending(tok);
            }
            return;
        }
        self.note_add(tok, p);
        self.note_del(tok, last);
    }

    fn fold_if_pending(&mut self, tok: &str) {
        let pending =
            self.dels.get(tok).map_or(0, |v| v.len()) + self.adds.get(tok).map_or(0, |v| v.len());
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
        let Some((key, base)) = self.postings.remove_entry(tok) else {
            if !adds.is_empty() {
                self.postings.insert(Arc::from(tok), adds);
            }
            return;
        };
        let list = merge_edits(&base, &dels, &adds);
        if !list.is_empty() {
            self.postings.insert(key, SmallVec::from_vec(list));
        }
    }

    fn fold_all(&mut self) {
        let terms: Vec<Arc<str>> = self.dels.keys().chain(self.adds.keys()).cloned().collect();
        for tok in terms {
            self.fold_term(&tok);
        }
    }
}

impl FtsIndex {
    pub fn empty(fields: &[String]) -> Self {
        Self {
            fields: fields.to_vec(),
            slots: build_fts_slots(fields),
            inner: Arc::new(FtsInner::default()),
        }
    }

    pub fn build(rows: &[Row], fields: &[String]) -> Self {
        let mut idx = Self::empty(fields);
        Arc::make_mut(&mut idx.inner).postings.reserve(rows.len());
        idx.append_slab(0, rows);
        idx
    }

    pub fn rebuild_in_place(&mut self, rows: &[Row], fields: &[String]) {
        if self.fields != fields {
            self.fields.clear();
            self.fields.extend_from_slice(fields);
            self.slots = build_fts_slots(fields);
        }
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.dels.clear();
            inner.adds.clear();
            for list in inner.postings.values_mut() {
                list.clear();
            }
            if inner.postings.capacity() < rows.len() {
                inner.postings.reserve(rows.len() - inner.postings.len());
            }
            self.append_slab(0, rows);
            Arc::make_mut(&mut self.inner)
                .postings
                .retain(|_, list| !list.is_empty());
        } else {
            self.inner = Arc::new(FtsInner {
                postings: FxHashMap::with_capacity_and_hasher(rows.len(), Default::default()),
                dels: FxHashMap::default(),
                adds: FxHashMap::default(),
            });
            self.append_slab(0, rows);
        }
    }

    /// Post a row whose position is new at the tail of the collection: written
    /// straight into the base list, which keeps appends allocation-cheap.
    #[allow(dead_code)]
    pub fn append_row(&mut self, row_idx: usize, row: &Row) {
        // Caller should provide scratch buffer if batching; single-row path uses one-off.
        let mut scratch = String::with_capacity(128);
        let inner = Arc::make_mut(&mut self.inner);
        inner.append_row_with_scratch(&mut self.slots, row_idx, row, &mut scratch);
    }

    /// Append fresh collection positions. Pending edits require the general path
    /// because a recycled tail position can still occur in an old base posting.
    pub fn append_slab(&mut self, start: usize, rows: &[Row]) {
        if rows.is_empty() {
            return;
        }
        if self.inner.adds.is_empty() && self.inner.dels.is_empty() {
            let first = &rows[0];
            // The slot mask below is computed from the first row's field positions, so
            // every row must have the same fields, not merely as many (rows with one
            // shape share it, which makes the common case a pointer comparison).
            let same_fields = |r: &Row| match (r.shape(), first.shape()) {
                (Some(a), Some(b)) if Arc::ptr_eq(a, b) => true,
                _ => r.len() == first.len() && r.keys().eq(first.keys()),
            };
            let uniform = rows[1..].iter().all(same_fields);
            if uniform && first.len() <= 64 {
                let has_snippet = self.fields.iter().any(|f| f == "snippet");
                let mut target_fields: SmallVec<[&str; 4]> =
                    self.fields.iter().map(|s| s.as_str()).collect();
                if !has_snippet {
                    target_fields.push("snippet");
                }
                let mut slot_mask: u64 = 0;
                for (pos, (k, _)) in first.iter().enumerate().take(64) {
                    if target_fields.contains(&k.as_str()) {
                        slot_mask |= 1u64 << pos;
                    }
                }
                if slot_mask == 0 {
                    return;
                }
                let inner = Arc::make_mut(&mut self.inner);
                let mut scratch = String::with_capacity(256);
                for (i, row) in rows.iter().enumerate() {
                    let row_idx = start + i;
                    let cells = row.cells_slice();
                    let mut m = slot_mask;
                    while m != 0 {
                        let pos = m.trailing_zeros() as usize;
                        m &= m - 1;
                        if let Some(text) = cells[pos].text() {
                            let lowered = lowercased(text, &mut scratch);
                            for_each_word(lowered, |tok| {
                                add_posting(&mut inner.postings, tok, row_idx);
                            });
                        }
                    }
                }
                return;
            }
            let inner = Arc::make_mut(&mut self.inner);
            let mut scratch = String::with_capacity(256);
            for (i, row) in rows.iter().enumerate() {
                inner.append_row_with_scratch(&mut self.slots, start + i, row, &mut scratch);
            }
        } else {
            let inner = Arc::make_mut(&mut self.inner);
            for (i, row) in rows.iter().enumerate() {
                collect_row_tokens(row, &mut self.slots);
                with_collected_tokens(|tok| {
                    inner.note_add(tok, start + i);
                });
            }
        }
    }

    /// Post a row at a position chosen by the writer rather than by the row
    /// count: an ordinary insert still lands as an append, while a position a
    /// base list already references (a swap-move or an undo restore) is
    /// registered through the delta set.
    pub fn insert_row(&mut self, row_idx: usize, row: &Row) {
        collect_row_tokens(row, &mut self.slots);
        let inner = Arc::make_mut(&mut self.inner);
        with_collected_tokens(|tok| {
            inner.note_add(tok, row_idx);
        });
    }

    /// Drop one row's postings, given the row that produced them: only its own
    /// tokens are visited, so the cost is per-row rather than per-vocabulary.
    /// Callers must invoke it while `row` still sits at `row_idx`.
    pub fn remove_row(&mut self, row_idx: usize, row: &Row) {
        collect_row_tokens(row, &mut self.slots);
        let inner = Arc::make_mut(&mut self.inner);
        with_collected_tokens(|tok| {
            inner.note_del(tok, row_idx);
        });
    }

    #[cfg(test)]
    fn note_add(&mut self, tok: &str, row_idx: usize) {
        Arc::make_mut(&mut self.inner).note_add(tok, row_idx);
    }

    #[cfg(test)]
    fn note_del(&mut self, tok: &str, row_idx: usize) {
        Arc::make_mut(&mut self.inner).note_del(tok, row_idx);
    }

    /// Fold every term; call before serializing so the file format stays base-only.
    pub fn fold_all(&mut self) {
        if self.inner.dels.is_empty() && self.inner.adds.is_empty() {
            return;
        }
        Arc::make_mut(&mut self.inner).fold_all();
    }

    /// Positions where `tok` currently matches, borrowed when no pending edits exist.
    pub fn matches_cow<'a>(&'a self, tok: &str) -> Option<std::borrow::Cow<'a, [usize]>> {
        let dels = self.inner.dels.get(tok).map(|v| v.as_slice()).unwrap_or(&[]);
        let adds = self.inner.adds.get(tok).map(|v| v.as_slice()).unwrap_or(&[]);
        if dels.is_empty() && adds.is_empty() {
            return self
                .inner
                .postings
                .get(tok)
                .map(|list| std::borrow::Cow::Borrowed(list.as_slice()));
        }
        let base = self
            .inner
            .postings
            .get(tok)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        Some(std::borrow::Cow::Owned(merge_edits(base, dels, adds)))
    }

    /// Positions where `tok` currently matches.
    #[allow(dead_code)]
    pub fn matches(&self, tok: &str) -> Option<Vec<usize>> {
        self.matches_cow(tok).map(std::borrow::Cow::into_owned)
    }

    /// `O(K)` fast-path for single-term lexical top-K queries when every matching row in
    /// the FTS posting list is guaranteed to achieve the uniform maximum score (`6`) in
    /// `lex_score_prepared` (tie-broken by ascending row index).
    ///
    /// Returns `Some(indices)` (up to `k` ascending live indices `< max_exclusive`) when:
    /// 1. `self.fields` only contains `"title"`, `"body"`, and/or `"snippet"`;
    /// 2. `query` lowers to a single non-empty, non-wildcard token `tok` with no leading,
    ///    trailing, or internal whitespace (`query.to_lowercase() == tok`);
    /// 3. every character in `tok` satisfies `c.is_alphanumeric() || c == '_'`.
    #[allow(dead_code)]
    pub(crate) fn single_alnum_term_first_k(
        &self,
        query: &str,
        k: usize,
        max_exclusive: usize,
    ) -> Option<Vec<usize>> {
        if !self
            .fields
            .iter()
            .all(|f| matches!(f.as_str(), "title" | "body" | "snippet"))
        {
            return None;
        }
        if query.is_empty() {
            return None;
        }
        let mut stack_buf = [0u8; 128];
        let mut heap_scratch = String::new();
        let tok: &str = if query.is_ascii() {
            let bytes = query.as_bytes();
            let mut has_upper = false;
            for &b in bytes {
                if !(b.is_ascii_alphanumeric() || b == b'_') {
                    return None;
                }
                has_upper |= b.is_ascii_uppercase();
            }
            if !has_upper {
                query
            } else if bytes.len() <= stack_buf.len() {
                for (dst, &src) in stack_buf[..bytes.len()].iter_mut().zip(bytes) {
                    *dst = src.to_ascii_lowercase();
                }
                // SAFETY: ASCII bytes remain valid UTF-8 after ASCII lowercasing.
                unsafe { std::str::from_utf8_unchecked(&stack_buf[..bytes.len()]) }
            } else {
                heap_scratch.push_str(query);
                heap_scratch.make_ascii_lowercase();
                heap_scratch.as_str()
            }
        } else {
            if query.chars().any(char::is_whitespace) {
                return None;
            }
            let lowered = lowercased(query, &mut heap_scratch);
            if lowered.is_empty()
                || !lowered
                    .chars()
                    .all(|c| !c.is_whitespace() && (c.is_alphanumeric() || c == '_'))
            {
                return None;
            }
            lowered
        };

        if k == 0 || max_exclusive == 0 {
            return Some(Vec::new());
        }

        let base = self
            .inner
            .postings
            .get(tok)
            .map_or(&[][..], |v| v.as_slice());
        let adds = self
            .inner
            .adds
            .get(tok)
            .map_or(&[][..], |v| v.as_slice());
        if base.is_empty() && adds.is_empty() {
            return Some(Vec::new());
        }
        let dels = self
            .inner
            .dels
            .get(tok)
            .map_or(&[][..], |v| v.as_slice());
        if dels.is_empty() && adds.is_empty() {
            let cap = base.len().min(k);
            let end = if cap == 0 || base[cap - 1] < max_exclusive {
                cap
            } else {
                base[..cap].partition_point(|&idx| idx < max_exclusive)
            };
            return Some(base[..end].to_vec());
        }

        let mut out = Vec::with_capacity(k.min(base.len() + adds.len()));
        let (mut i, mut j, mut a_idx) = (0, 0, 0);
        while out.len() < k {
            while i < base.len() {
                let b_val = base[i];
                if b_val >= max_exclusive {
                    i = base.len();
                    break;
                }
                while j < dels.len() && dels[j] < b_val {
                    j += 1;
                }
                if j < dels.len() && dels[j] == b_val {
                    i += 1;
                } else {
                    break;
                }
            }
            let next_base = base.get(i).copied().filter(|&x| x < max_exclusive);
            let next_add = adds.get(a_idx).copied().filter(|&x| x < max_exclusive);
            let picked = match (next_base, next_add) {
                (Some(bv), Some(av)) if bv < av => {
                    i += 1;
                    bv
                }
                (Some(bv), Some(av)) if bv == av => {
                    i += 1;
                    a_idx += 1;
                    bv
                }
                (Some(_), Some(av)) => {
                    a_idx += 1;
                    av
                }
                (Some(bv), None) => {
                    i += 1;
                    bv
                }
                (None, Some(av)) => {
                    a_idx += 1;
                    av
                }
                (None, None) => break,
            };
            out.push(picked);
        }
        Some(out)
    }

    /// Re-point a row's postings from `from` to `to`. The token set is the row's
    /// own and does not change, so it is computed once instead of per edit —
    /// which is what a swap-move needs, since it moves one row between slots.
    pub fn move_row(&mut self, from: usize, to: usize, row: &Row) {
        collect_row_tokens(row, &mut self.slots);
        let inner = Arc::make_mut(&mut self.inner);
        with_collected_tokens(|tok| {
            inner.note_del(tok, from);
            inner.note_add(tok, to);
        });
    }

    /// Re-point postings after a swap-remove where row `p` was removed and
    /// the tail row at `last` was moved into slot `p`.
    ///
    /// The doomed row at `p` is replaced by `tail`, and the vacated `last` slot
    /// is dropped. Tokens common to both `doomed` and `tail` remain present at
    /// slot `p` without any edit.
    pub fn swap_remove_row(&mut self, p: usize, doomed: &Row, last: usize, tail: &Row) {
        let inner = Arc::make_mut(&mut self.inner);
        let slots = &mut self.slots;
        FTS_SCRATCH.with(|pair_cell| {
            let mut pair = pair_cell.borrow_mut();
            let FtsScratchPair { a, b } = &mut *pair;
            a.collect(doomed, slots);
            b.collect(tail, slots);

            let (mut d_pos, mut t_pos) = (0, 0);
            while d_pos < a.tokens.len() && t_pos < b.tokens.len() {
                let d_str = a.token_str(d_pos);
                let t_str = b.token_str(t_pos);
                match d_str.as_bytes().cmp(t_str.as_bytes()) {
                    std::cmp::Ordering::Less => {
                        inner.note_del(d_str, p);
                        d_pos += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        inner.note_move_from_tail(t_str, p, last);
                        t_pos += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        inner.note_del_tail(t_str, last);
                        d_pos += 1;
                        t_pos += 1;
                    }
                }
            }
            while d_pos < a.tokens.len() {
                let d_str = a.token_str(d_pos);
                inner.note_del(d_str, p);
                d_pos += 1;
            }
            while t_pos < b.tokens.len() {
                let t_str = b.token_str(t_pos);
                inner.note_move_from_tail(t_str, p, last);
                t_pos += 1;
            }
        });
    }

    /// Re-point one row's postings after an in-place update: only tokens that
    /// entered or left the row touch a posting list, so the cost is per-row
    /// rather than per-vocabulary.
    pub fn sync_row(&mut self, row_idx: usize, before: &Row, after: &Row) {
        if self
            .slots
            .iter_mut()
            .all(|slot| slot.get(before) == slot.get(after))
        {
            return;
        }
        let inner = Arc::make_mut(&mut self.inner);
        let slots = &mut self.slots;
        FTS_SCRATCH.with(|pair_cell| {
            let mut pair = pair_cell.borrow_mut();
            let FtsScratchPair { a, b } = &mut *pair;
            a.collect(before, slots);
            b.collect(after, slots);

            let (mut old_pos, mut new_pos) = (0, 0);
            while old_pos < a.tokens.len() && new_pos < b.tokens.len() {
                let old_str = a.token_str(old_pos);
                let new_str = b.token_str(new_pos);
                match old_str.as_bytes().cmp(new_str.as_bytes()) {
                    std::cmp::Ordering::Less => {
                        inner.note_del(old_str, row_idx);
                        old_pos += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        inner.note_add(new_str, row_idx);
                        new_pos += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        old_pos += 1;
                        new_pos += 1;
                    }
                }
            }
            while old_pos < a.tokens.len() {
                let old_str = a.token_str(old_pos);
                inner.note_del(old_str, row_idx);
                old_pos += 1;
            }
            while new_pos < b.tokens.len() {
                let new_str = b.token_str(new_pos);
                inner.note_add(new_str, row_idx);
                new_pos += 1;
            }
        });
    }

    /// Union of posting lists for query tokens (any-token match, then residual score).
    pub fn candidate_idxs(&self, query: &str) -> Vec<usize> {
        let mut scratch = String::new();
        let lowered = lowercased(query, &mut scratch);
        let mut terms: SmallVec<[&str; 8]> = SmallVec::new();

        for_each_word(lowered, |tok| {
            if let Some(prefix) = tok.strip_suffix('*') {
                if !prefix.is_empty() {
                    let mut matched_terms: SmallVec<[&str; 16]> = SmallVec::new();
                    for k in self.inner.postings.keys().chain(self.inner.adds.keys()) {
                        if k.starts_with(prefix) {
                            matched_terms.push(k.as_ref());
                        }
                    }
                    matched_terms.sort_unstable();
                    matched_terms.dedup();
                    terms.extend(matched_terms);
                }
            } else {
                terms.push(tok);
            }
        });

        if terms.is_empty() {
            return Vec::new();
        }

        terms.sort_unstable();
        terms.dedup();

        let mut matched_lists: SmallVec<[std::borrow::Cow<'_, [usize]>; 8]> =
            SmallVec::with_capacity(terms.len());
        for term in terms {
            if let Some(list) = self.matches_cow(term)
                && !list.is_empty()
            {
                matched_lists.push(list);
            }
        }

        if matched_lists.is_empty() {
            return Vec::new();
        }
        if matched_lists.len() == 1 {
            return matched_lists.pop().unwrap().into_owned();
        }

        matched_lists.sort_unstable_by_key(|l| l.len());
        let mut buf_a = matched_lists[0].to_vec();
        let mut buf_b = Vec::new();

        for next_list in &matched_lists[1..] {
            union_sorted_into(&buf_a, next_list.as_ref(), &mut buf_b);
            std::mem::swap(&mut buf_a, &mut buf_b);
        }

        buf_a
    }

    pub fn encode(&self, generation: u64, nrows: u64) -> Vec<u8> {
        let mut terms: Vec<(&Arc<str>, &SmallVec<[usize; 2]>)> =
            self.inner.postings.iter().collect();
        terms.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let fields_bytes: usize = self.fields.iter().map(|f| 4 + f.len()).sum();
        let terms_bytes: usize = terms
            .iter()
            .map(|(tok, list)| 8 + tok.len() + list.len() * 4)
            .sum();
        let exact_cap = 25 + fields_bytes + 4 + terms_bytes + 8;
        let mut out = Vec::with_capacity(exact_cap);
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
        out.extend_from_slice(&FTS_CRC_MAGIC);
        let crc = crate::crc::hardware_crc32c(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        debug_assert_eq!(out.len(), exact_cap);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<(u64, u64, Self), Error> {
        // Verify the trailer before parsing anything: a damaged length field could
        // otherwise make the parser consume the trailer as data and skip the check.
        let body = match bytes.len().checked_sub(8) {
            Some(t) if bytes[t..t + 4] == FTS_CRC_MAGIC => {
                let stored = u32::from_le_bytes(bytes[t + 4..].try_into().unwrap());
                if crate::crc::hardware_crc32c(&bytes[..t + 4]) != stored {
                    return Err(io_err("corrupt fts checksum"));
                }
                &bytes[..t]
            }
            // Files written before the trailer existed.
            _ => bytes,
        };
        let bytes = body;
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
        let mut fields = Vec::with_capacity(nfields.min((bytes.len() - i) / 4));
        for _ in 0..nfields {
            fields.push(read_str(bytes, &mut i)?);
        }
        let nterms = read_u32(bytes, &mut i)? as usize;
        let mut postings =
            FxHashMap::with_capacity_and_hasher(nterms.min((bytes.len() - i) / 8), Default::default());
        for _ in 0..nterms {
            let tok = read_arc_str(bytes, &mut i)?;
            let n = read_u32(bytes, &mut i)? as usize;
            let mut list = SmallVec::with_capacity(n.min((bytes.len() - i) / 4));
            for _ in 0..n {
                let idx_u32 = read_u32(bytes, &mut i)?;
                if (idx_u32 as u64) >= nrows {
                    return Err(io_err("corrupt fts posting"));
                }
                list.push(idx_u32 as usize);
            }
            postings.insert(tok, list);
        }
        if i != bytes.len() {
            return Err(io_err("corrupt fts trailer"));
        }
        let slots = build_fts_slots(&fields);
        Ok((
            generation,
            nrows,
            Self {
                fields,
                slots,
                inner: Arc::new(FtsInner {
                    postings,
                    dels: FxHashMap::default(),
                    adds: FxHashMap::default(),
                }),
            },
        ))
    }

    /// Summarize vocabulary size, posting lengths, and pending delta debt (`LIN-0918`, `LIN-0989`).
    pub fn vocabulary_stats(&self) -> FtsVocabularyStats {
        let mut unique_terms = self.inner.postings.len();
        let mut vocabulary_bytes: usize = self.inner.postings.keys().map(|k| k.len()).sum();
        let mut total_base_postings = 0usize;
        let mut max_posting_len = 0usize;
        for list in self.inner.postings.values() {
            total_base_postings += list.len();
            if list.len() > max_posting_len {
                max_posting_len = list.len();
            }
        }
        for (tok, list) in &self.inner.adds {
            if !self.inner.postings.contains_key(tok.as_ref()) && !list.is_empty() {
                unique_terms += 1;
                vocabulary_bytes += tok.len();
                if list.len() > max_posting_len {
                    max_posting_len = list.len();
                }
            }
        }
        let pending_add_terms = self.inner.adds.len();
        let pending_del_terms = self.inner.dels.len();
        let add_entries: usize = self.inner.adds.values().map(|v| v.len()).sum();
        let del_entries: usize = self.inner.dels.values().map(|v| v.len()).sum();
        FtsVocabularyStats {
            unique_terms,
            vocabulary_bytes,
            total_base_postings,
            pending_add_terms,
            pending_del_terms,
            compaction_debt_entries: add_entries + del_entries,
            max_posting_len,
        }
    }

    /// Verify sorted base lists, disjoint `adds`/`dels` sets, and optional row-index bounds (`LIN-0990`, `LIN-0991`).
    pub fn check_invariants(&self, row_limit: Option<usize>) -> FtsInvariantReport {
        let mut unsorted_base_terms = 0usize;
        let mut overlap_add_del_terms = 0usize;
        let mut out_of_bounds_postings = 0usize;

        for list in self.inner.postings.values() {
            if list.is_empty() || list.windows(2).any(|w| w[0] >= w[1]) {
                unsorted_base_terms += 1;
            }
        }

        for (tok, add_list) in &self.inner.adds {
            if let Some(del_list) = self.inner.dels.get(tok.as_ref()) {
                let (mut i, mut j) = (0, 0);
                let mut overlaps = false;
                while i < add_list.len() && j < del_list.len() {
                    match add_list[i].cmp(&del_list[j]) {
                        std::cmp::Ordering::Less => i += 1,
                        std::cmp::Ordering::Greater => j += 1,
                        std::cmp::Ordering::Equal => {
                            overlaps = true;
                            break;
                        }
                    }
                }
                if overlaps {
                    overlap_add_del_terms += 1;
                }
            }
        }

        if let Some(limit) = row_limit {
            let mut terms: Vec<&str> = self
                .inner
                .postings
                .keys()
                .chain(self.inner.adds.keys())
                .map(AsRef::as_ref)
                .collect();
            terms.sort_unstable();
            terms.dedup();
            for term in terms {
                if let Some(live) = self.matches_cow(term) {
                    let pos = live.partition_point(|&idx| idx < limit);
                    out_of_bounds_postings += live.len() - pos;
                }
            }
        }

        let valid = unsorted_base_terms == 0
            && overlap_add_del_terms == 0
            && out_of_bounds_postings == 0;
        FtsInvariantReport {
            valid,
            unsorted_base_terms,
            overlap_add_del_terms,
            out_of_bounds_postings,
        }
    }

    /// Compare live effective postings against a clean rebuild over `rows` (`LIN-0992`).
    pub fn diff_against_rebuild(&self, rows: &[Row]) -> FtsRebuildDiff {
        let rebuilt = Self::build(rows, &self.fields);
        let mut all_terms: Vec<&str> = self
            .inner
            .postings
            .keys()
            .chain(self.inner.adds.keys())
            .chain(rebuilt.inner.postings.keys())
            .map(AsRef::as_ref)
            .collect();
        all_terms.sort_unstable();
        all_terms.dedup();

        let mut only_in_live = Vec::new();
        let mut only_in_rebuilt = Vec::new();
        for term in all_terms {
            let live = self.matches_cow(term);
            let live_slice = live.as_deref().unwrap_or(&[]);
            let reb = rebuilt.matches_cow(term);
            let reb_slice = reb.as_deref().unwrap_or(&[]);
            if live_slice == reb_slice {
                continue;
            }
            let diff_live = sorted_diff(live_slice, reb_slice);
            if !diff_live.is_empty() {
                only_in_live.push((term.to_string(), diff_live));
            }
            let diff_reb = sorted_diff(reb_slice, live_slice);
            if !diff_reb.is_empty() {
                only_in_rebuilt.push((term.to_string(), diff_reb));
            }
        }
        FtsRebuildDiff {
            identical: only_in_live.is_empty() && only_in_rebuilt.is_empty(),
            only_in_live,
            only_in_rebuilt,
        }
    }

    /// Build an [`FtsIndex`] enforcing a hard vocabulary byte ceiling and reporting progress (`LIN-0910`, `LIN-0919`).
    pub fn build_with_limits(
        fields: &[String],
        rows: &[Row],
        max_vocab_bytes: usize,
        on_progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Self, Error> {
        let total = rows.len();
        let mut idx = Self::empty(fields);
        if total == 0 {
            on_progress(0, 0);
            return Ok(idx);
        }
        let inner = Arc::make_mut(&mut idx.inner);
        inner.postings.reserve(total);
        let mut vocab_bytes = 0usize;
        let mut scratch = String::with_capacity(256);
        let step = (total / 16).clamp(1, 256);

        for (row_idx, row) in rows.iter().enumerate() {
            for slot in &mut idx.slots {
                if let Some(text) = slot.get(row).and_then(Cell::text) {
                    let lowered = lowercased(text, &mut scratch);
                    let mut limit_err: Option<Error> = None;
                    for_each_word(lowered, |tok| {
                        if limit_err.is_some() {
                            return;
                        }
                        if let Some(list) = inner.postings.get_mut(tok) {
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
                            let next_bytes = vocab_bytes.saturating_add(tok.len());
                            if next_bytes > max_vocab_bytes {
                                limit_err = Some(Error::resource_limit(format!(
                                    "FTS vocabulary bytes {next_bytes} exceeds max_vocab_bytes {max_vocab_bytes}"
                                )));
                                return;
                            }
                            vocab_bytes = next_bytes;
                            inner.postings.insert(Arc::from(tok), smallvec![row_idx]);
                        }
                    });
                    if let Some(err) = limit_err {
                        return Err(err);
                    }
                }
            }
            if (row_idx + 1) % step == 0 || row_idx + 1 == total {
                on_progress(row_idx + 1, total);
            }
        }
        Ok(idx)
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

fn read_str_ref<'a>(b: &'a [u8], i: &mut usize) -> Result<&'a str, Error> {
    let n = read_u32(b, i)? as usize;
    let end = i.saturating_add(n);
    if end > b.len() {
        return Err(io_err("truncated fts token"));
    }
    let s = std::str::from_utf8(&b[*i..end]).map_err(io_err)?;
    *i = end;
    Ok(s)
}

fn read_str(b: &[u8], i: &mut usize) -> Result<String, Error> {
    Ok(read_str_ref(b, i)?.to_string())
}

fn read_arc_str(b: &[u8], i: &mut usize) -> Result<Arc<str>, Error> {
    Ok(Arc::from(read_str_ref(b, i)?))
}

#[allow(dead_code)]
pub fn tokenize(text: &str) -> Vec<String> {
    let mut scratch = String::new();
    let lowered = lowercased(text, &mut scratch);
    let mut out = Vec::new();
    for_each_word(lowered, |t| out.push(t.to_string()));
    out
}

#[inline]
fn is_combining_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05C7}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

#[inline]
fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || is_combining_mark(c)
}

#[inline]
fn stem_ascii_suffix_in_place(tok: &mut String) {
    if !tok.is_ascii() {
        return;
    }
    if let Some(stem) = tok.strip_suffix("ing") {
        if stem.len() >= 3 {
            let new_len = stem.len();
            tok.truncate(new_len);
            let bytes = tok.as_bytes();
            if bytes.len() >= 4
                && bytes[bytes.len() - 1] == bytes[bytes.len() - 2]
                && !matches!(bytes[bytes.len() - 1], b'l' | b's' | b'z')
            {
                tok.pop();
            }
        }
    } else if let Some(stem) = tok.strip_suffix("ed") {
        if stem.len() >= 3 {
            let new_len = stem.len();
            tok.truncate(new_len);
        }
    } else if let Some(stem) = tok.strip_suffix('s')
        && !stem.ends_with('s')
        && stem.len() >= 3
    {
        let new_len = stem.len();
        tok.truncate(new_len);
    }
}

/// Tokenize `text` with full span offsets, contextual Unicode lowercasing (Greek final `ς`),
/// combining-mark retention, stopword filtering, optional ASCII suffix stemming, and token caps
/// (`LIN-0897`..`LIN-0904`, `LIN-0909`, `LIN-0912`, `LIN-0917`).
pub fn inspect_tokens(text: &str, cfg: &TokenizerConfig) -> Vec<TokenSpan> {
    if text.is_empty() || cfg.max_tokens_per_doc == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut span_start: Option<usize> = None;

    let emit_span = |start: usize, end: usize, out: &mut Vec<TokenSpan>| -> bool {
        let raw = &text[start..end];
        let mut tok = raw.to_lowercase();
        if let Some(ref sw) = cfg.stopwords
            && sw.contains(tok.as_str())
        {
            return true;
        }
        if cfg.ascii_stem_s_ing_ed {
            stem_ascii_suffix_in_place(&mut tok);
            if let Some(ref sw) = cfg.stopwords
                && sw.contains(tok.as_str())
            {
                return true;
            }
        }
        if tok.len() < cfg.min_token_bytes
            || (cfg.max_token_bytes > 0 && tok.len() > cfg.max_token_bytes)
        {
            return true;
        }
        let position = out.len();
        out.push(TokenSpan {
            token: tok,
            byte_start: start,
            byte_end: end,
            position,
        });
        out.len() < cfg.max_tokens_per_doc
    };

    for (idx, ch) in text.char_indices() {
        if is_token_char(ch) {
            if span_start.is_none() {
                span_start = Some(idx);
            }
        } else if let Some(start) = span_start.take()
            && !emit_span(start, idx, &mut out)
        {
            return out;
        }
    }
    if let Some(start) = span_start {
        let _ = emit_span(start, text.len(), &mut out);
    }
    out
}

#[inline]
fn gallop_lower_bound(slice: &[usize], start: usize, target: usize) -> usize {
    if start >= slice.len() || slice[start] >= target {
        return start;
    }
    let mut lo = start;
    let mut step = 1usize;
    let mut hi = (start + step).min(slice.len());
    while hi < slice.len() && slice[hi] < target {
        lo = hi;
        step = step.saturating_mul(2);
        hi = start.saturating_add(step).min(slice.len());
    }
    let bound = (hi + 1).min(slice.len());
    lo + slice[lo..bound].partition_point(|&x| x < target)
}

/// Shortest-list-first sorted posting intersection using exponential galloping search (`LIN-0925`, `LIN-0926`).
pub fn intersect_postings_galloping(lists: &[&[usize]]) -> Vec<usize> {
    if lists.is_empty() || lists.iter().any(|l| l.is_empty()) {
        return Vec::new();
    }
    if lists.len() == 1 {
        let mut single = lists[0].to_vec();
        single.dedup();
        return single;
    }
    let mut ordered: SmallVec<[&[usize]; 8]> = lists.iter().copied().collect();
    ordered.sort_unstable_by_key(|s| s.len());

    let head = ordered[0];
    let rest = &ordered[1..];
    let mut cursors: SmallVec<[usize; 8]> = smallvec![0usize; rest.len()];
    let mut out = Vec::with_capacity(head.len());

    let mut i0 = 0usize;
    'outer: while i0 < head.len() {
        let mut candidate = head[i0];
        let mut matched_all = true;
        for (r_idx, &other) in rest.iter().enumerate() {
            let pos = gallop_lower_bound(other, cursors[r_idx], candidate);
            cursors[r_idx] = pos;
            if pos >= other.len() {
                break 'outer;
            }
            if other[pos] > candidate {
                candidate = other[pos];
                matched_all = false;
                break;
            }
        }
        if matched_all {
            if out.last() != Some(&candidate) {
                out.push(candidate);
            }
            i0 += 1;
            while i0 < head.len() && head[i0] == candidate {
                i0 += 1;
            }
            for (r_idx, &other) in rest.iter().enumerate() {
                while cursors[r_idx] < other.len() && other[cursors[r_idx]] == candidate {
                    cursors[r_idx] += 1;
                }
            }
        } else {
            i0 = gallop_lower_bound(head, i0 + 1, candidate);
        }
    }
    out
}

fn sorted_diff(a: &[usize], b: &[usize]) -> Vec<usize> {
    if b.is_empty() {
        return a.to_vec();
    }
    let mut out = Vec::new();
    let mut j = 0usize;
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

/// ASCII bytes matching Unicode `White_Space` (`\t`, `\n`, `\x0b`, `\x0c`, `\r`, `' '`).
#[inline]
pub(crate) fn is_ascii_unicode_ws(b: u8) -> bool {
    b == b' ' || b.wrapping_sub(b'\t') < 5
}

/// Split `s` on Unicode whitespace, using a fast byte loop when `s` is ASCII.
#[inline]
pub(crate) fn for_each_word<'a>(s: &'a str, mut f: impl FnMut(&'a str)) {
    if s.is_ascii() {
        let bytes = s.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            while i < len && is_ascii_unicode_ws(bytes[i]) {
                i += 1;
            }
            if i >= len {
                break;
            }
            let start = i;
            i += 1;
            while i < len && !is_ascii_unicode_ws(bytes[i]) {
                i += 1;
            }
            // SAFETY: `s` is ASCII, so every byte offset is a valid UTF-8 boundary.
            f(unsafe { std::str::from_utf8_unchecked(&bytes[start..i]) });
        }
    } else {
        for tok in s.split_whitespace() {
            f(tok);
        }
    }
}

/// Text of every field an index tokenizes: the `fts` fields, plus `snippet`
/// when it is not one of them (lex scoring reads it).
#[cfg(test)]
fn fts_texts<'a: 'b, 'b>(row: &'a Row, fields: &'b [String]) -> impl Iterator<Item = &'a str> + 'b {
    let snippet = if fields.iter().any(|f| f == "snippet") {
        None
    } else {
        crate::store::row_text(row, "snippet")
    };
    fields
        .iter()
        .filter_map(move |f| crate::store::row_text(row, f))
        .chain(snippet)
}

/// `text` lowercased, borrowed from `text` itself when lowering is a no-op.
#[inline]
pub(crate) fn lowercased<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    // Single auto-vectorized byte pass for the common already-lowercase ASCII case.
    if !text.bytes().any(|b| b >= 0x80 || b.is_ascii_uppercase()) {
        return text;
    }

    if text.is_ascii() {
        scratch.clear();
        scratch.reserve(text.len());
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    // `str::to_lowercase`, not per character: it applies the contextual rules (a final
    // capital sigma becomes `ς`), as query lowering and lex scoring do. Per-character
    // lowering turned `ΟΔΟΣ` into `οδοσ`, so it never matched a stored `οδος`.
    scratch.clear();
    scratch.push_str(&text.to_lowercase());
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

/// `a ∪ b` for two sorted, duplicate-free lists into `out`, collapsing any overlap between them.
fn union_sorted_into(a: &[usize], b: &[usize], out: &mut Vec<usize>) {
    out.clear();
    out.reserve(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let va = a[i];
        let vb = b[j];
        match va.cmp(&vb) {
            std::cmp::Ordering::Less => {
                out.push(va);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(vb);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                out.push(va);
                i += 1;
                j += 1;
            }
        }
    }
    if i < a.len() {
        out.extend_from_slice(&a[i..]);
    }
    if j < b.len() {
        out.extend_from_slice(&b[j..]);
    }
}

/// `a ∪ b` for two sorted lists, collapsing any overlap between them.
#[allow(dead_code)]
fn union_sorted(a: &[usize], b: &[usize]) -> Vec<usize> {
    if b.is_empty() {
        return a.to_vec();
    }
    if a.is_empty() {
        return b.to_vec();
    }
    let mut out = Vec::with_capacity(a.len() + b.len());
    union_sorted_into(a, b, &mut out);
    out
}

/// Insert into a sorted, unique pending list; merges rely on this invariant.
fn push_pending(
    map: &mut FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
    postings: &FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
    tok: &str,
    row_idx: usize,
) -> usize {
    if let Some(list) = map.get_mut(tok) {
        if list.last().is_some_and(|&last| row_idx > last) {
            list.push(row_idx);
        } else if let Err(pos) = list.binary_search(&row_idx) {
            list.insert(pos, row_idx);
        }
        list.len()
    } else {
        let key = if let Some((k, _)) = postings.get_key_value(tok) {
            Arc::clone(k)
        } else {
            Arc::from(tok)
        };
        map.insert(key, smallvec![row_idx]);
        1
    }
}

/// Drop `row_idx` from a term's pending list, reporting whether it was there.
/// A cancelled entry means the base list already holds the right answer.
fn cancel(map: &mut FxHashMap<Arc<str>, SmallVec<[usize; 2]>>, tok: &str, row_idx: usize) -> bool {
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
fn add_posting(
    postings: &mut FxHashMap<Arc<str>, SmallVec<[usize; 2]>>,
    tok: &str,
    row_idx: usize,
) {
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
        postings.insert(Arc::from(tok), smallvec![row_idx]);
    }
}

struct FtsTokenBuf {
    lower: String,
    scratch: String,
    tokens: Vec<(u32, u32)>,
}

impl FtsTokenBuf {
    fn new() -> Self {
        Self {
            lower: String::with_capacity(512),
            scratch: String::with_capacity(512),
            tokens: Vec::with_capacity(64),
        }
    }

    #[inline]
    fn token_str(&self, idx: usize) -> &str {
        let (s, e) = self.tokens[idx];
        // SAFETY: every `(s, e)` span was produced by `for_each_word` over `self.scratch`.
        unsafe { std::str::from_utf8_unchecked(&self.scratch.as_bytes()[s as usize..e as usize]) }
    }

    fn collect(&mut self, row: &Row, slots: &mut [OwnedFieldSlot]) {
        collect_tokens_into_buffers(
            row,
            slots,
            &mut self.scratch,
            &mut self.lower,
            &mut self.tokens,
        );
    }
}

struct FtsScratchPair {
    a: FtsTokenBuf,
    b: FtsTokenBuf,
}

thread_local! {
    static FTS_SCRATCH: std::cell::RefCell<FtsScratchPair> = std::cell::RefCell::new(FtsScratchPair {
        a: FtsTokenBuf::new(),
        b: FtsTokenBuf::new(),
    });
}

fn collect_tokens_into_buffers(
    row: &Row,
    slots: &mut [OwnedFieldSlot],
    scratch: &mut String,
    lower_scratch: &mut String,
    tokens: &mut Vec<(u32, u32)>,
) {
    scratch.clear();
    tokens.clear();

    for slot in slots {
        if let Some(text) = slot.get(row).and_then(Cell::text) {
            if !scratch.is_empty() {
                scratch.push(' ');
            }
            let lowered = lowercased(text, lower_scratch);
            scratch.push_str(lowered);
        }
    }
    let base_ptr = scratch.as_ptr() as usize;
    for_each_word(scratch, |word| {
        let word_offset = (word.as_ptr() as usize) - base_ptr;
        tokens.push((word_offset as u32, (word_offset + word.len()) as u32));
    });

    let bytes = scratch.as_bytes();
    tokens.sort_unstable_by(|&(s1, e1), &(s2, e2)| {
        bytes[s1 as usize..e1 as usize].cmp(&bytes[s2 as usize..e2 as usize])
    });
    tokens.dedup_by(|&mut (s1, e1), &mut (s2, e2)| {
        bytes[s1 as usize..e1 as usize] == bytes[s2 as usize..e2 as usize]
    });
}

fn collect_row_tokens(row: &Row, slots: &mut [OwnedFieldSlot]) {
    FTS_SCRATCH.with(|pair_cell| {
        pair_cell.borrow_mut().a.collect(row, slots);
    });
}

fn with_collected_tokens(mut f: impl FnMut(&str)) {
    FTS_SCRATCH.with(|pair_cell| {
        let pair = pair_cell.borrow();
        for idx in 0..pair.a.tokens.len() {
            f(pair.a.token_str(idx));
        }
    });
}

#[cfg(test)]
fn row_tokens<'a>(row: &'a Row, fields: &[String]) -> Vec<std::borrow::Cow<'a, str>> {
    let mut out = Vec::with_capacity(fields.len() * 10);
    thread_local! {
        static SCRATCH: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(512));
    }
    for text in fts_texts(row, fields) {
        if text
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_uppercase())
        {
            out.extend(text.split_whitespace().map(std::borrow::Cow::Borrowed));
        } else {
            SCRATCH.with(|sc| {
                let mut scratch = sc.borrow_mut();
                let lowered = lowercased(text, &mut scratch);
                for tok in lowered.split_whitespace() {
                    out.push(std::borrow::Cow::Owned(tok.to_string()));
                }
            });
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
#[allow(dead_code)]
pub fn write_fts(
    data: &Path,
    name: &str,
    idx: &mut FtsIndex,
    generation: u64,
    nrows: u64,
) -> Result<(), Error> {
    write_fts_ext(data, name, idx, generation, nrows, true)
}

fn write_fts_encoded(
    data: &Path,
    name: &str,
    body: &[u8],
    sync_parent: bool,
) -> Result<(), Error> {
    let dir = fts_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = fts_path(data, name);
    let tmp = path.with_extension("bin.tmp");
    crate::persist::write_through_tmp(&path, &tmp, body)?;
    if sync_parent {
        crate::persist::sync_dir_checked(&dir)?;
    }
    Ok(())
}

pub(crate) fn write_fts_ext(
    data: &Path,
    name: &str,
    idx: &mut FtsIndex,
    generation: u64,
    nrows: u64,
    sync_parent: bool,
) -> Result<(), Error> {
    idx.fold_all();
    let body = idx.encode(generation, nrows);
    write_fts_encoded(data, name, &body, sync_parent)
}

pub(crate) fn write_fts_readonly(
    data: &Path,
    name: &str,
    idx: &FtsIndex,
    generation: u64,
    nrows: u64,
    sync_parent: bool,
) -> Result<(), Error> {
    let body = idx.encode(generation, nrows);
    write_fts_encoded(data, name, &body, sync_parent)
}

pub fn read_fts(data: &Path, name: &str) -> Result<(u64, u64, FtsIndex), Error> {
    let path = fts_path(data, name);
    let mut f = File::open(&path).map_err(io_err)?;
    let cap = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut buf = Vec::with_capacity(cap);
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
    use crate::store::{Cell, row_text};

    #[test]
    fn snippet_mutations_match_rebuilt_index() {
        for fields in [vec!["title".into()], vec!["title".into(), "snippet".into()]] {
            let before = Row::from([
                ("title", Cell::text_arc("shared")),
                ("snippet".into(), Cell::text_arc("OLD shared ЁЖ")),
            ]);
            let after = Row::from([
                ("title", Cell::text_arc("shared")),
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
            assert_eq!(
                index.inner.postings,
                FtsIndex::build(&[before], &fields).inner.postings
            );
        }
    }

    #[test]
    fn sync_token_sets_match_rebuild() {
        let fields = vec!["title".into(), "body".into()];
        for old_mask in 0..32 {
            for new_mask in 0..32 {
                let row = |mask: usize| {
                    Row::from([(
                        "title",
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
                assert_eq!(
                    index.inner.postings,
                    FtsIndex::build(&[after], &fields).inner.postings
                );
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
                .map(|_| Row::from([("title", Cell::text_arc(text))]))
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
                    "title",
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
                    ("title", Cell::text_arc(title)),
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
        assert_eq!(slab.inner.postings, single.inner.postings);
        assert_eq!(slab.inner.adds, single.inner.adds);
        assert_eq!(slab.inner.dels, single.inner.dels);
    }

    #[test]
    fn appended_slabs_match_general_insert_after_tail_recycling() {
        let fields = vec!["title".into(), "body".into()];
        let rows = (0..300)
            .map(|i| {
                Row::from([
                    (
                        "title",
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
        let replacement = Row::from([("title", Cell::text_arc("replacement"))]);
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
        assert_eq!(fast.inner.postings, reference.inner.postings);
    }

    #[test]
    fn repeated_term_removal_and_reinsertion_matches_current_rows() {
        let fields = vec!["title".to_owned()];
        let row = Row::from([("title", Cell::text_arc("foo"))]);
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
                    "title",
                    Cell::text_arc(if i % 2 == 0 { "WAL ёж" } else { "wal other" }),
                )])
            })
            .collect::<Vec<_>>();
        let mut index = FtsIndex::build(&rows, &fields);
        for i in (0..40).step_by(3) {
            index.remove_row(i, &rows[i]);
            rows[i] = Row::from([("title", Cell::text_arc("new ЁЖ"))]);
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
            ("title", Cell::text_arc("wal wal ёж WAL")),
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
        row.insert("title", Cell::text_arc("durable wal"));
        row.insert("body", Cell::text_arc("checkpoint postings"));
        let fields = vec![String::from("title"), String::from("body")];
        let index = FtsIndex::build(&[row], &fields);
        let encoded = index.encode(7, 1);
        assert_eq!(&encoded[encoded.len() - 8..encoded.len() - 4], b"LCRC");
        let (generation, rows, decoded) = FtsIndex::decode(&encoded).unwrap();
        assert_eq!((generation, rows), (7, 1));
        assert_eq!(decoded.fields, fields);
        assert_eq!(decoded.candidate_idxs("wal"), vec![0]);
        assert!(FtsIndex::decode(&encoded[..encoded.len() - 1]).is_err());
        assert!(FtsIndex::decode(&encoded[..encoded.len() - 5]).is_err());
        // Flipping a byte inside the header/body fails checksum verification.
        let mut flipped = encoded.clone();
        flipped[10] ^= 0x5a;
        assert!(FtsIndex::decode(&flipped).is_err());
        // Earlier trailer-less `LIN\x05` files still decode; truncating their body still fails.
        let legacy = &encoded[..encoded.len() - 8];
        let (_, _, legacy_dec) = FtsIndex::decode(legacy).unwrap();
        assert_eq!(legacy_dec.candidate_idxs("wal"), vec![0]);
        assert!(FtsIndex::decode(&legacy[..legacy.len() - 1]).is_err());
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
    use crate::store::Cell;

    #[test]
    fn fts_matches_cow_and_ping_pong_union() {
        let rows = vec![
            Row::from([("text", Cell::from_text("apple banana cherry"))]),
            Row::from([("text", Cell::from_text("banana cherry date"))]),
            Row::from([("text", Cell::from_text("cherry date elderberry"))]),
        ];
        let index = FtsIndex::build(&rows, &["text".to_string()]);

        // matches_cow returns Borrowed when no pending edits exist
        let cow = index.matches_cow("banana").unwrap();
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)));
        assert_eq!(cow.as_ref(), &[0, 1]);

        // Multi-term candidate union
        let candidates = index.candidate_idxs("apple date");
        assert_eq!(candidates, &[0, 1, 2]);

        // Prefix candidate union
        let prefix_cand = index.candidate_idxs("elder*");
        assert_eq!(prefix_cand, &[2]);
    }
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
                let reference = text.to_lowercase();
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
        Arc::make_mut(&mut index.inner)
            .postings
            .insert("wal".into(), (0..600).collect());
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
        Arc::make_mut(&mut index.inner)
            .postings
            .insert("wal".into(), (0..10).collect());
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

    #[test]
    fn implicit_snippet_indexing_and_two_cursor_token_diff() {
        let mut r0 = Row::new();
        r0.insert("title", Cell::from_text("alpha beta"));
        r0.insert("snippet", Cell::from_text("implicit_tok shared_tok"));
        let rows = vec![r0.clone()];

        // Even when "snippet" is omitted from declared FTS fields, FtsIndex indexes it implicitly.
        let mut index = FtsIndex::build(&rows, &[String::from("title")]);
        assert_eq!(index.matches("alpha"), Some(vec![0]));
        assert_eq!(index.matches("implicit_tok"), Some(vec![0]));
        assert_eq!(index.matches("shared_tok"), Some(vec![0]));

        // Update only snippet: removes "implicit_tok", keeps "shared_tok", adds "fresh_tok".
        let mut r1 = r0.clone();
        r1.insert("snippet", Cell::from_text("shared_tok fresh_tok"));
        index.sync_row(0, &r0, &r1);
        assert_eq!(
            index.matches("implicit_tok").unwrap_or_default(),
            Vec::<usize>::new()
        );
        assert_eq!(index.matches("shared_tok"), Some(vec![0]));
        assert_eq!(index.matches("fresh_tok"), Some(vec![0]));

        // Remove the last remaining terms by clearing the row and folding pending edits.
        let empty = Row::new();
        index.sync_row(0, &r1, &empty);
        assert_eq!(
            index.matches("alpha").unwrap_or_default(),
            Vec::<usize>::new()
        );
        assert_eq!(
            index.matches("shared_tok").unwrap_or_default(),
            Vec::<usize>::new()
        );
        assert_eq!(
            index.matches("fresh_tok").unwrap_or_default(),
            Vec::<usize>::new()
        );
        index.fold_all();
        assert_eq!(index.matches("alpha"), None);
    }

    #[test]
    fn single_alnum_term_first_k_matches_lex_scoring_and_cow_sharing() {
        use crate::eval::{LexQuery, lex_score_prepared};

        let fields = vec!["title".to_string(), "body".to_string()];
        let mut rows: Vec<Row> = (0..200)
            .map(|i| {
                Row::from([
                    (
                        "title",
                        Cell::from_text(if i % 3 == 0 {
                            "rust_wal storage"
                        } else {
                            "other header"
                        }),
                    ),
                    (
                        "body",
                        Cell::from_text(if i % 5 == 0 {
                            "ЁЖ ΟΔΟΣ rust_wal"
                        } else {
                            "plain body"
                        }),
                    ),
                ])
            })
            .collect();

        let mut index = FtsIndex::build(&rows, &fields);

        // CoW clone shares inner backing map until mutated.
        let snapshot = index.clone();
        assert!(Arc::ptr_eq(&index.inner, &snapshot.inner));

        // Mutate `index` with pending edits (remove + insert + sync) while `snapshot` stays intact.
        index.remove_row(0, &rows[0]);
        rows[0] = Row::from([("title", Cell::from_text("removed"))]);
        index.insert_row(0, &rows[0]);

        let old_7 = rows[7].clone();
        rows[7] = Row::from([("title", Cell::from_text("rust_wal fresh_hit ЁЖ"))]);
        index.sync_row(7, &old_7, &rows[7]);

        assert!(!Arc::ptr_eq(&index.inner, &snapshot.inner));
        assert_eq!(
            snapshot.single_alnum_term_first_k("rust_wal", 3, 200),
            Some(vec![0, 3, 5])
        );

        for q in ["rust_wal", "RUST_WAL", "ёж", "ЁЖ", "οδος", "ΟΔΟΣ", "absent_tok"] {
            for k in [0, 1, 5, 10, 500] {
                for max_excl in [0, 10, 50, rows.len()] {
                    let got = index.single_alnum_term_first_k(q, k, max_excl).unwrap();
                    let lq = LexQuery::new(q);
                    let mut expected: Vec<(i64, usize)> = index
                        .candidate_idxs(q)
                        .into_iter()
                        .filter(|&i| i < max_excl)
                        .filter_map(|i| {
                            let s = lex_score_prepared(&rows[i], &lq);
                            (s > 0).then_some((s, i))
                        })
                        .collect();
                    expected.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                    expected.truncate(k);
                    let exp_idxs: Vec<usize> = expected.into_iter().map(|(_, i)| i).collect();
                    assert_eq!(got, exp_idxs, "q={q:?} k={k} max={max_excl}");
                }
            }
        }

        // Queries that must decline the fast-path (`None`).
        for rejected in ["", "  rust_wal", "rust_wal ", "rust wal", "rust*", "rust-wal", "a!b"] {
            assert_eq!(index.single_alnum_term_first_k(rejected, 10, 200), None, "{rejected:?}");
        }

        // Non-standard indexed fields outside {title, body, snippet} must also decline (`None`).
        let custom_fts = FtsIndex::build(
            &[Row::from([("author", Cell::from_text("alice"))])],
            &["author".to_string()],
        );
        assert_eq!(custom_fts.single_alnum_term_first_k("alice", 10, 1), None);
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
