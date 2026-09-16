//! Full-text posting lists for `search lex` / hybrid lex-half.
//!
//! Built from catalog fields with `fts: true`. Tokens are lowercase whitespace
//! words (aligned with [`crate::exec`] lex scoring). Rebuild-on-open / after
//! writes; not a separate durable blob in v1.

use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

use crate::store::{Row, row_text};

/// Inverted index: token → sorted unique row indices.
#[derive(Debug, Clone, Default)]
pub struct FtsIndex {
    pub fields: Vec<String>,
    postings: FxHashMap<String, Vec<usize>>,
}

impl FtsIndex {
    pub fn build(rows: &[Row], fields: &[String]) -> Self {
        let mut idx = Self {
            fields: fields.to_vec(),
            postings: FxHashMap::default(),
        };
        for (i, row) in rows.iter().enumerate() {
            idx.insert_row(i, row);
        }
        idx
    }

    pub fn insert_row(&mut self, row_idx: usize, row: &Row) {
        for tok in row_tokens(row, &self.fields) {
            let list = self.postings.entry(tok).or_default();
            if list.last().copied() != Some(row_idx) {
                match list.binary_search(&row_idx) {
                    Ok(_) => {}
                    Err(pos) => list.insert(pos, row_idx),
                }
            }
        }
    }

    /// Remove row_idx from all postings (after delete / before re-index).
    pub fn remove_row_idx(&mut self, row_idx: usize) {
        for list in self.postings.values_mut() {
            if let Ok(pos) = list.binary_search(&row_idx) {
                list.remove(pos);
            }
        }
        self.postings.retain(|_, v| !v.is_empty());
    }

    /// Union of posting lists for query tokens (any-token match, then residual score).
    pub fn candidate_idxs(&self, query: &str) -> Vec<usize> {
        let mut set = BTreeSet::new();
        for tok in tokenize(query) {
            if let Some(list) = self.postings.get(&tok) {
                set.extend(list.iter().copied());
            }
        }
        set.into_iter().collect()
    }
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

fn row_tokens(row: &Row, fields: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for f in fields {
        if let Some(t) = row_text(row, f) {
            for tok in tokenize(t) {
                out.insert(tok);
            }
        }
    }
    // Also index snippet when present (lex_score reads it) even if not fts-flagged.
    if !fields.iter().any(|f| f == "snippet") {
        if let Some(t) = row_text(row, "snippet") {
            for tok in tokenize(t) {
                out.insert(tok);
            }
        }
    }
    out
}
