//! Full-text posting lists for `search lex` / hybrid lex-half.
//!
//! Built from catalog fields with `fts: true`. Tokens are lowercase whitespace
//! words (aligned with [`crate::exec`] lex scoring). Durable stores persist
//! postings as `fts/<collection>.bin` on checkpoint (`LIN\x05`); open loads
//! them and only rebuilds missing / mismatched collections. WAL replay then
//! updates postings incrementally.

use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::persist::io_err;
use crate::store::{Row, row_text};

pub const FTS_DIR: &str = "fts";
pub const FTS_MAGIC: [u8; 4] = *b"LIN\x05";
const FTS_VERSION: u8 = 1;

/// Inverted index: token → sorted unique row indices.
#[derive(Debug, Clone, Default)]
pub struct FtsIndex {
    pub fields: Vec<String>,
    postings: FxHashMap<String, Vec<usize>>,
}

impl FtsIndex {
    pub fn empty(fields: &[String]) -> Self {
        Self {
            fields: fields.to_vec(),
            postings: FxHashMap::default(),
        }
    }

    pub fn build(rows: &[Row], fields: &[String]) -> Self {
        let mut idx = Self::empty(fields);
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
        Ok((generation, nrows, Self { fields, postings }))
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
    if !fields.iter().any(|f| f == "snippet")
        && let Some(t) = row_text(row, "snippet")
    {
        for tok in tokenize(t) {
            out.insert(tok);
        }
    }
    out
}

pub fn fts_dir(data: &Path) -> PathBuf {
    data.join(FTS_DIR)
}

pub fn fts_path(data: &Path, name: &str) -> PathBuf {
    fts_dir(data).join(format!("{name}.bin"))
}

pub fn write_fts(
    data: &Path,
    name: &str,
    idx: &FtsIndex,
    generation: u64,
    nrows: u64,
) -> Result<(), Error> {
    let dir = fts_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = fts_path(data, name);
    let tmp = path.with_extension("bin.tmp");
    let body = idx.encode(generation, nrows);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)
        .map_err(io_err)?;
    file.write_all(&body).map_err(io_err)?;
    file.sync_all().map_err(io_err)?;
    drop(file);
    fs::rename(&tmp, &path).map_err(io_err)?;
    Ok(())
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
    let dir = fts_dir(data);
    if !dir.exists() {
        return Ok(());
    }
    for ent in fs::read_dir(&dir).map_err(io_err)? {
        let ent = ent.map_err(io_err)?;
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if !keep.iter().any(|k| k == &stem) {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
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
}
