//! Cold / mmap-backed collection bodies (P2).
//!
//! On checkpoint with `OpenOpts.cold`, large collections are also written to
//! `cold/<name>.bin` (MessagePack `Vec<Row>`, magic `LIN\x03`) as a decode
//! cache. The durable `snapshot` (`LIN\x04` MessagePack) stays **self-contained**
//! with the same rows inlined. Open may keep cold cols mmapped and page-in
//! lazily on first access.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// Memory-map of a MessagePack-encoded `Vec<Row>` with lazy materialize.
pub struct ColdCol {
    mmap: Mmap,
    hot: OnceLock<Vec<Row>>,
}

impl std::fmt::Debug for ColdCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColdCol")
            .field("bytes", &self.mmap.len())
            .field("materialized", &self.hot.get().is_some())
            .finish()
    }
}

impl ColdCol {
    pub fn rows(&self) -> Result<&[Row], Error> {
        if self.hot.get().is_none() {
            let decoded = self.decode()?;
            let _ = self.hot.set(decoded);
        }
        Ok(self.hot.get().map(|v| v.as_slice()).unwrap_or(&[]))
    }

    pub fn into_rows(mut self) -> Result<Vec<Row>, Error> {
        if let Some(v) = self.hot.take() {
            return Ok(v);
        }
        self.decode()
    }

    fn decode(&self) -> Result<Vec<Row>, Error> {
        // Detect if compressed by checking for flag byte + magic vs plain magic
        let (data_offset, is_compressed): (usize, bool) = if self.mmap.len() >= 5 
            && self.mmap[0] != COLD_MAGIC[0] 
            && self.mmap[1..5] == COLD_MAGIC 
        {
            // Compressed format: flag at 0, magic at 1-4
            (5, true)
        } else if self.mmap.len() >= 4 && self.mmap[0..4] == COLD_MAGIC {
            // Uncompressed format: magic at 0-3
            (4, false)
        } else {
            return Err(io_err("bad cold magic"));
        };
        
        if is_compressed {
            match self.mmap[0] {
                0x01 => {
                    #[cfg(feature = "compress-flate")]
                    {
                        use flate2::read::GzDecoder;
                        use std::io::Read;
                        
                        let compressed_data = &self.mmap[data_offset..];
                        let mut decoder = GzDecoder::new(compressed_data);
                        let mut result = Vec::new();
                        decoder.read_to_end(&mut result).map_err(io_err)?;
                        rmp_serde::from_slice(&result).map_err(io_err)
                    }
                    #[cfg(not(feature = "compress-flate"))]
                    Err(io_err("gzip compression not enabled"))
                }
                0x02 => {
                    #[cfg(feature = "compress-lz4")]
                    {
                        use lz4::block::{decompress, Context};
                        
                        let compressed_data = &self.mmap[data_offset..];
                        let mut ctx = Context::new();
                        let decompressed = decompress(compressed_data, Some(usize::MAX), &mut ctx).map_err(io_err)?;
                        rmp_serde::from_slice(&decompressed).map_err(io_err)
                    }
                    #[cfg(not(feature = "compress-lz4"))]
                    Err(io_err("lz4 compression not enabled"))
                }
                _ => Err(io_err("unknown compression format")),
            }
        } else {
            rmp_serde::from_slice(&self.mmap[data_offset..]).map_err(io_err)
        }
    }
}

/// Write `rows` as a cold file (atomic replace) with optional compression
#[cfg(any(feature = "compress-flate", feature = "compress-lz4"))]
pub fn write_cold(data: &Path, name: &str, rows: &[Row]) -> Result<(), Error> {
    use std::io::Write;
    
    let dir = cold_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = cold_path(data, name);
    let tmp = path.with_extension("bin.tmp");

    let mut body = Vec::with_capacity(rows.len() * 64);
    
    #[cfg(feature = "compress-flate")]
    {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        
        // Encode rows to compressed format
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        rmp_serde::encode::write_named(&mut encoder, rows).map_err(io_err)?;
        let compressed = encoder.finish().map_err(io_err)?;
        
        // Add compression flag
        body.extend_from_slice(&[0x01]); // Compressed flag
        body.extend_from_slice(&COLD_MAGIC);
        body.extend_from_slice(&compressed);
    }
    
    #[cfg(all(not(feature = "compress-flate"), feature = "compress-lz4"))]
    {
        use lz4::block::{compress, Context};
        
        let encoded = rmp_serde::to_vec_named(rows).map_err(io_err)?;
        
        let mut ctx = Context::new();
        match compress(&encoded, Some(9), &mut ctx) {
            Ok(compressed) => {
                body.push(0x02); // LZ4 flag
                body.extend_from_slice(&COLD_MAGIC);
                body.extend_from_slice(&compressed);
            }
            Err(_) => return Err(Error::runtime("lz4 compression failed")),
        }
    }
    
    #[cfg(not(any(feature = "compress-flate", feature = "compress-lz4")))]
    {
        body.extend_from_slice(&COLD_MAGIC);
        rmp_serde::encode::write_named(&mut body, rows).map_err(io_err)?;
    }

    crate::persist::write_through_tmp(&path, &tmp, &body)
}

/// Write `rows` as a cold file without compression (for non-compression builds)
#[cfg(not(any(feature = "compress-flate", feature = "compress-lz4")))]
pub fn write_cold(data: &Path, name: &str, rows: &[Row]) -> Result<(), Error> {
    let dir = cold_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = cold_path(data, name);
    let tmp = path.with_extension("bin.tmp");

    let mut body = Vec::with_capacity(rows.len() * 64);
    body.extend_from_slice(&COLD_MAGIC);
    rmp_serde::encode::write_named(&mut body, rows).map_err(io_err)?;

    crate::persist::write_through_tmp(&path, &tmp, &body)
}

pub fn map_cold(data: &Path, name: &str) -> Result<ColdCol, Error> {
    let path = cold_path(data, name);
    let file = File::open(&path).map_err(io_err)?;
    let mmap = unsafe { Mmap::map(&file) }.map_err(io_err)?;
    
    // Check both compressed (magic at offset 1) and uncompressed (magic at offset 0)
    let valid = mmap.len() >= 5 && mmap[1..5] == COLD_MAGIC || 
                mmap.len() >= 4 && mmap[0..4] == COLD_MAGIC;
    if !valid {
        return Err(io_err(format!("bad cold file: {}", path.display())));
    }
    Ok(ColdCol {
        mmap,
        hot: OnceLock::new(),
    })
}

/// Drop stale cold files not listed in `keep`.
pub fn prune_cold(data: &Path, keep: &[String]) -> Result<(), Error> {
    crate::persist::prune_bins(&cold_dir(data), keep)
}
