//! Cold / mmap-backed collection bodies (P2).
//!
//! On checkpoint with `OpenOpts.cold`, large collections are also written to
//! `cold/<name>.bin` (MessagePack `Vec<Row>`, magic `LIN\x03`) as a decode
//! cache. The durable `snapshot` (`LIN\x04` MessagePack) stays **self-contained**
//! with the same rows inlined — backup/copy never depends on cold files alone.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::error::Error;
use crate::persist::io_err;
use crate::store::Row;

pub const COLD_DIR: &str = "cold";
pub const COLD_MAGIC: [u8; 4] = *b"LIN\x03";
/// Spill to mmap when collection has at least this many rows.
pub const COLD_MIN_ROWS: usize = 32;

pub fn cold_dir(data: &Path) -> PathBuf {
    data.join(COLD_DIR)
}

pub fn cold_path(data: &Path, name: &str) -> PathBuf {
    cold_dir(data).join(format!("{name}.bin"))
}

/// Memory-map of a MessagePack-encoded `Vec<Row>`.
pub struct ColdCol {
    mmap: Mmap,
}

impl std::fmt::Debug for ColdCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColdCol")
            .field("bytes", &self.mmap.len())
            .finish()
    }
}

impl ColdCol {
    pub fn into_rows(self) -> Result<Vec<Row>, Error> {
        if self.mmap.len() < 4 || self.mmap[0..4] != COLD_MAGIC {
            return Err(io_err("bad cold magic"));
        }
        rmp_serde::from_slice(&self.mmap[4..]).map_err(io_err)
    }
}

/// Write `rows` as a cold file (atomic replace).
pub fn write_cold(data: &Path, name: &str, rows: &[Row]) -> Result<(), Error> {
    let dir = cold_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = cold_path(data, name);
    let tmp = path.with_extension("bin.tmp");

    let mut body = Vec::with_capacity(rows.len() * 64);
    body.extend_from_slice(&COLD_MAGIC);
    rmp_serde::encode::write_named(&mut body, rows).map_err(io_err)?;

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

pub fn map_cold(data: &Path, name: &str) -> Result<ColdCol, Error> {
    let path = cold_path(data, name);
    let file = File::open(&path).map_err(io_err)?;
    let mmap = unsafe { Mmap::map(&file) }.map_err(io_err)?;
    if mmap.len() < 4 || mmap[0..4] != COLD_MAGIC {
        return Err(io_err(format!("bad cold file: {}", path.display())));
    }
    Ok(ColdCol { mmap })
}

/// Drop stale cold files not listed in `keep`.
pub fn prune_cold(data: &Path, keep: &[String]) -> Result<(), Error> {
    let dir = cold_dir(data);
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
