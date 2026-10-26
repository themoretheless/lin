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

/// Memory-map of a MessagePack-encoded `Vec<Row>` with lazy materialize.
pub struct ColdCol {
    mmap: Mmap,
    hot: OnceLock<Vec<Row>>,
}

/// Compression strategy for cold storage files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum CompressionStrategy {
    /// Always use maximum compression (flate/gzip)
    Maximum,
    /// Always use fast decompression (lz4)
    Speed,
    /// Auto-select based on data entropy analysis
    #[default]
    Adaptive,
}

/// Calculate Shannon entropy of byte distribution (0.0-8.0 for bytes)
/// Higher entropy = less compressible data
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    // Count byte frequencies
    let mut freq = [0usize; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for count in freq {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

/// Choose optimal compression based on entropy analysis of already-encoded bytes.
pub fn choose_codec_bytes(
    encoded: &[u8],
    strategy: CompressionStrategy,
) -> (bool, CompressionCodec) {
    if encoded.is_empty() {
        return (false, CompressionCodec::None);
    }

    let entropy = calculate_entropy(encoded);

    let want_flate = match strategy {
        CompressionStrategy::Maximum => true,
        CompressionStrategy::Speed => false,
        CompressionStrategy::Adaptive => entropy < 3.0,
    };

    if want_flate {
        #[cfg(feature = "compress-flate")]
        return (true, CompressionCodec::Flate);
        #[cfg(all(not(feature = "compress-flate"), feature = "compress-lz4"))]
        return (true, CompressionCodec::Lz4);
        #[cfg(not(any(feature = "compress-flate", feature = "compress-lz4")))]
        return (false, CompressionCodec::None);
    } else {
        #[cfg(feature = "compress-lz4")]
        return (true, CompressionCodec::Lz4);
        #[cfg(all(not(feature = "compress-lz4"), feature = "compress-flate"))]
        return (true, CompressionCodec::Flate);
        #[cfg(not(any(feature = "compress-flate", feature = "compress-lz4")))]
        return (false, CompressionCodec::None);
    }
}

/// Choose optimal compression based on entropy analysis
/// Returns (compression_type, recommended_compressor)
#[allow(dead_code)]
pub fn choose_codec(data: &[Row], strategy: CompressionStrategy) -> (bool, CompressionCodec) {
    let encoded = rmp_serde::to_vec_named(data).unwrap_or_default();
    choose_codec_bytes(&encoded, strategy)
}

/// Compression codec identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompressionCodec {
    None,
    Flate, // gzip (maximum compression)
    Lz4,   // lz4 (fast decompression)
}

impl std::fmt::Display for CompressionCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionCodec::None => write!(f, "none"),
            CompressionCodec::Flate => write!(f, "flate"),
            CompressionCodec::Lz4 => write!(f, "lz4"),
        }
    }
}

pub fn cold_dir(data: &Path) -> PathBuf {
    data.join(COLD_DIR)
}

pub fn cold_path(data: &Path, name: &str) -> PathBuf {
    cold_dir(data).join(format!("{name}.bin"))
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

    /// Create a new ColdCol
    pub fn new(mmap: Mmap) -> Self {
        Self {
            mmap,
            hot: OnceLock::new(),
        }
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

                        let compressed_data = self
                            .mmap
                            .get(data_offset..)
                            .ok_or_else(|| io_err("cold mmap offset out of bounds"))?;
                        let decoder = GzDecoder::new(compressed_data);
                        let mut limited = decoder.take(256 * 1024 * 1024 + 1);
                        let mut result = Vec::new();
                        limited.read_to_end(&mut result).map_err(io_err)?;
                        if result.len() > 256 * 1024 * 1024 {
                            return Err(io_err(
                                "cold mmap payload exceeds maximum decompression limit",
                            ));
                        }
                        rmp_serde::from_slice(&result).map_err(io_err)
                    }
                    #[cfg(not(feature = "compress-flate"))]
                    Err(io_err("gzip compression not enabled"))
                }
                0x02 => {
                    #[cfg(feature = "compress-lz4")]
                    {
                        let compressed_data = self
                            .mmap
                            .get(data_offset..)
                            .ok_or_else(|| io_err("cold mmap offset out of bounds"))?;
                        let decompressed =
                            lz4::block::decompress(compressed_data, Some(256 * 1024 * 1024))
                                .map_err(io_err)?;
                        rmp_serde::from_slice(&decompressed).map_err(io_err)
                    }
                    #[cfg(not(feature = "compress-lz4"))]
                    Err(io_err("lz4 compression not enabled"))
                }
                _ => Err(io_err("unknown compression format")),
            }
        } else {
            let data = self
                .mmap
                .get(data_offset..)
                .ok_or_else(|| io_err("cold mmap offset out of bounds"))?;
            rmp_serde::from_slice(data).map_err(io_err)
        }
    }
}

/// Write `rows` as a cold file (atomic replace) with optional compression
#[cfg(any(feature = "compress-flate", feature = "compress-lz4"))]
pub fn write_cold(data: &Path, name: &str, rows: &[Row]) -> Result<(), Error> {
    let dir = cold_dir(data);
    fs::create_dir_all(&dir).map_err(io_err)?;
    let path = cold_path(data, name);
    let tmp = path.with_extension("bin.tmp");

    // Serialize once
    let encoded = rmp_serde::to_vec_named(rows).map_err(io_err)?;
    let (is_compressed, codec) = choose_codec_bytes(&encoded, CompressionStrategy::Adaptive);

    let mut body = Vec::new();

    if is_compressed {
        match codec {
            CompressionCodec::Flate => {
                #[cfg(feature = "compress-flate")]
                {
                    use flate2::Compression;
                    use flate2::write::GzEncoder;
                    use std::io::Write;

                    body.push(0x01); // Flag for Flate
                    body.extend_from_slice(&COLD_MAGIC);
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                    encoder.write_all(&encoded).map_err(io_err)?;
                    let compressed = encoder.finish().map_err(io_err)?;
                    body.extend_from_slice(&compressed);
                }
                #[cfg(not(feature = "compress-flate"))]
                return Err(io_err("flate compression not enabled"));
            }
            CompressionCodec::Lz4 => {
                #[cfg(feature = "compress-lz4")]
                {
                    use lz4::block::{CompressionMode, compress};

                    body.push(0x02); // Flag for Lz4
                    body.extend_from_slice(&COLD_MAGIC);
                    let compressed =
                        compress(&encoded, Some(CompressionMode::DEFAULT), true).map_err(io_err)?;
                    body.extend_from_slice(&compressed);
                }
                #[cfg(not(feature = "compress-lz4"))]
                return Err(io_err("lz4 compression not enabled"));
            }
            CompressionCodec::None => {
                body.extend_from_slice(&COLD_MAGIC);
                body.extend_from_slice(&encoded);
            }
        }
    } else {
        body.extend_from_slice(&COLD_MAGIC);
        body.extend_from_slice(&encoded);
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

    #[cfg(unix)]
    let _ = mmap.advise(memmap2::Advice::Sequential);

    // Check both compressed (magic at offset 1) and uncompressed (magic at offset 0)
    let valid =
        mmap.len() >= 5 && mmap[1..5] == COLD_MAGIC || mmap.len() >= 4 && mmap[0..4] == COLD_MAGIC;
    if !valid {
        return Err(io_err(format!("bad cold file: {}", path.display())));
    }
    Ok(ColdCol::new(mmap))
}

/// Drop stale cold files not listed in `keep`.
pub fn prune_cold(data: &Path, keep: &[String]) -> Result<(), Error> {
    crate::persist::prune_bins(&cold_dir(data), keep)
}
