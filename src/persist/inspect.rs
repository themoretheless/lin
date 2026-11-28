//! Byte-level WAL and snapshot format specification, decoder capability matrix,
//! non-mutating corruption/frame inspectors, and dry-run crash-recovery diagnostics.
//!
//! # Byte-Level On-Disk Frame & Snapshot Layouts (`LIN-0067`)
//!
//! All multi-byte integer fields in WAL frames and snapshot envelopes are encoded in
//! **little-endian** byte order.
//!
//! ## 1. WAL Record Envelopes (`<dir>/log`)
//!
//! ### v3 Checksummed Envelope (`LOG_MAGIC_CHECKSUM = b"LIN\x06"`) — Current Default Writer
//! ```text
//! Offset  Size   Field             Description
//! ------  -----  ----------------  -------------------------------------------------------------
//! 0       4      magic             ASCII/binary tag `b"LIN\x06"` (`[0x4c, 0x49, 0x4e, 0x06]`)
//! 4       4      envelope_len      u32 LE (`1 + payload.len() + 4`), bounded by `MAX_RECORD` (16 MiB)
//! 8       1      codec_tag         Inner codec identifier (`1..=5`, see table below)
//! 9       N      encoded_payload   Codec-specific payload (`N = envelope_len - 5`)
//! 9 + N   4      crc32c_trailer    u32 LE CRC32C over `&frame[8 .. 9 + N]` (`codec_tag || payload`);
//!                                  readers also accept legacy IEEE CRC32 fallback.
//! ```
//!
//! #### Inner Codec Tags (`codec_tag` at byte offset 8 of `LIN\x06`)
//! - `1` (`LOG_MAGIC_V1` / `\x01`): Named MessagePack (`rmp_serde`) encoding of [`super::LogRecord`].
//! - `2` (`LOG_MAGIC_V2` / `\x02`): Dense raw columnar hot pack:
//!   - `V2_FACTS_BULK = 1`:
//!     `[0] = 0x01`, `[1..9] = u64 LE gen`, `[9..17] = u64 LE next_id`, `[17..21] = u32 LE count`,
//!     followed by `count` triples `(s, p, o)` each encoded as `u32 LE byte_len + UTF-8 bytes`.
//!   - `V2_INSERT_COLS = 2`:
//!     `[0] = 0x02`, `[1..9] = u64 LE gen`, `[9..17] = u64 LE next_id`, `[17..21] = u32 LE row_count`,
//!     `[21..25] = u32 LE edge_count`, length-prefixed `collection`, `u32 LE col_count`, typed column
//!     tags/streams (text, i64, f64, bool, time, dense f32 vectors, null), and `(rel, from, to)` edges.
//! - `3` (`LOG_MAGIC_SPARSE` / `\x03`): Columnar `V2_INSERT_COLS` layout where sparse float vectors
//!   encode `(u32 LE dim_plus_one, u32 LE nnz, [(u32 LE index, u32 LE f32_bits); nnz])` bounded by
//!   `MAX_SPARSE_DECODED_BYTES` (64 MiB).
//! - `4` (Gzip MessagePack): Gzip stream (`flate2`, requires `compress-wal` or `compress-flate`)
//!   decompressing to MessagePack(`LogRecord`), bounded by `MAX_SPARSE_DECODED_BYTES` (64 MiB).
//! - `5` (`LOG_MAGIC_DICTIONARY` / `\x05`): Columnar `V2_INSERT_COLS` layout adding deduplicated
//!   dictionary vector encoding alongside dense and sparse vector columns.
//!
//! ### Historical Unchecksummed Envelopes (Dual-Read Compatibility)
//! - **`LOG_MAGIC_V1` (`b"LIN\x01"`)**:
//!   `[0..4] = b"LIN\x01"`, `[4..8] = u32 LE payload_len`, `[8..8+len] = MessagePack(LogRecord)`.
//! - **`LOG_MAGIC_V2` (`b"LIN\x02"`)**:
//!   `[0..4] = b"LIN\x02"`, `[4..8] = u32 LE payload_len`, `[8..8+len] = V2_FACTS_BULK | V2_INSERT_COLS`.
//! - **Legacy JSON (`v0`)**:
//!   `[0..4] = u32 LE json_len` (distinguished from `"LIN*"` magic), `[4..4+len] = UTF-8 JSON(LogRecord)`.
//!
//! ## 2. Snapshot & Backup Envelopes (`<dir>/snapshot`)
//!
//! ### Chunked Parallel Checksummed Format (`0x03` + `SNAPSHOT_MAGIC = b"LIN\x04"` + `b"LCRC"`)
//! ```text
//! Offset  Size   Field             Description
//! ------  -----  ----------------  -------------------------------------------------------------
//! 0       1      version           `0x03` (chunked columnar/MessagePack format)
//! 1       4      magic             `b"LIN\x04"` (`SNAPSHOT_MAGIC`; note `b"LIN\x07"` is ship v7 wire magic)
//! 5       4      header_len        u32 LE byte length of `SnapshotHeader` (<= `MAX_SNAPSHOT_SECTION_BYTES`)
//! 9       H      header_msgpack    Named MessagePack(`SnapshotHeader`: gen, embed_id, next_id,
//!                                  catalog_hash, log_offset, edges, extra_*, cold_collections)
//! 9 + H   4      chunk_count       u32 LE number of collection chunks
//! ...     var    chunks            Repeated: `u16 LE name_len`, `UTF-8 name`, `u32 LE data_len`,
//!                                  `chunk_data` (`b"LCOL" + codec_magic + cols` or MsgPack, optionally Zstd)
//! end-8   4      crc_magic         `b"LCRC"` trailer marker (optional on pre-trailer `0x03` files)
//! end-4   4      crc32c            u32 LE CRC32C over `&snapshot[0 .. end - 4]` (including `b"LCRC"`)
//! ```
//!
//! Legacy/optional snapshot prefixes also accepted by readers (when corresponding features are enabled):
//! - `b"LIN\x04"` + MessagePack(`Snapshot`) (uncompressed single-stream `0x00`)
//! - `0x01` + `b"LIN\x04"` + Gzip(MessagePack(`Snapshot`)) (`compress-flate`)
//! - `0x02` + `b"LIN\x04"` + LZ4(MessagePack(`Snapshot`)) (`compress-lz4`)
//! - `0x04` + `b"LIN\x04"` + Zstd(MessagePack(`Snapshot`)) (`compress-zstd`)
//! - Raw UTF-8 JSON(`Snapshot`)

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Error;

use super::snapshot::{
    MAX_SNAPSHOT_COLLECTIONS, MAX_SNAPSHOT_EDGES, MAX_SNAPSHOT_SECTION_BYTES, SNAPSHOT_CRC_MAGIC,
    decode_snapshot_bytes_checked, validate_msgpack_header_limits,
};
use super::types::{
    Head, LOG_MAGIC_CHECKSUM, LOG_MAGIC_V1, LOG_MAGIC_V2, LogRecord, MAX_RECORD, Pack,
    SNAPSHOT_MAGIC, SnapshotHeader,
};
use super::wal::{
    decode_frame, head_path, io_err, log_path, snapshot_path, validate_data_dir,
};

/// Explicit compile-time codec capability discovery for WAL frames and snapshots (`LIN-0066`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecCapabilities;

#[cfg(any(feature = "compress-wal", feature = "compress-flate"))]
const SUPPORTED_WAL_CODECS: &[u8] = &[1, 2, 3, 4, 5];

#[cfg(not(any(feature = "compress-wal", feature = "compress-flate")))]
const SUPPORTED_WAL_CODECS: &[u8] = &[1, 2, 3, 5];

const SNAP_MAGIC_CHUNKED_V3: &[u8] = b"\x03LIN\x04";
const SNAP_MAGIC_RAW_MSGPACK: &[u8] = b"LIN\x04";
#[cfg(feature = "compress-flate")]
const SNAP_MAGIC_GZIP: &[u8] = b"\x01LIN\x04";
#[cfg(feature = "compress-lz4")]
const SNAP_MAGIC_LZ4: &[u8] = b"\x02LIN\x04";
#[cfg(feature = "compress-zstd")]
const SNAP_MAGIC_ZSTD: &[u8] = b"\x04LIN\x04";

const SUPPORTED_SNAPSHOT_MAGICS: &[&[u8]] = &[
    SNAP_MAGIC_CHUNKED_V3,
    SNAP_MAGIC_RAW_MSGPACK,
    #[cfg(feature = "compress-flate")]
    SNAP_MAGIC_GZIP,
    #[cfg(feature = "compress-lz4")]
    SNAP_MAGIC_LZ4,
    #[cfg(feature = "compress-zstd")]
    SNAP_MAGIC_ZSTD,
];

impl CodecCapabilities {
    /// Returns the sorted slice of `LIN\x06` WAL inner codec tags supported by this build.
    pub const fn supported_wal_codecs() -> &'static [u8] {
        SUPPORTED_WAL_CODECS
    }

    /// Returns the binary magic prefixes supported by this build's snapshot decoder
    /// (in addition to legacy UTF-8 JSON snapshots).
    pub const fn supported_snapshot_magics() -> &'static [&'static [u8]] {
        SUPPORTED_SNAPSHOT_MAGICS
    }

    /// True if `codec` is a supported `LIN\x06` WAL codec tag in this build.
    pub fn supports_wal_codec(codec: u8) -> bool {
        Self::supported_wal_codecs().contains(&codec)
    }

    /// True if `bytes` begins with a supported binary snapshot magic prefix in this build.
    pub fn supports_snapshot_magic(bytes: &[u8]) -> bool {
        Self::supported_snapshot_magics()
            .iter()
            .any(|magic| bytes.starts_with(magic))
    }
}

/// Metadata for a single decoded WAL frame (`LIN-0068`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalFrameInfo {
    pub frame_index: usize,
    pub byte_offset: u64,
    pub byte_len: u64,
    pub codec: u8,
    pub r#gen: u64,
    pub next_id: u64,
    pub pack_kind: String,
    pub crc_valid: bool,
}

/// Non-mutating byte-level inspection report for a WAL byte stream or file (`LIN-0068`, `LIN-1575`, `LIN-1718`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalInspectionReport {
    pub frames: Vec<WalFrameInfo>,
    pub valid_prefix_bytes: u64,
    pub damaged_offset: Option<u64>,
    pub damaged_reason: Option<String>,
    pub failed_frame_hexdump: Option<String>,
}

/// Non-mutating inspection report for a snapshot file (`LIN-0068`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInspectionReport {
    pub exists: bool,
    pub byte_len: u64,
    pub format_version: Option<u8>,
    pub crc_trailer_present: bool,
    pub crc_valid: bool,
    pub r#gen: Option<u64>,
    pub collection_count: usize,
    pub cold_collections: Vec<String>,
}

/// Non-mutating crash-recovery dry-run report combining `head`, `snapshot`, and `log` (`LIN-0151`, `LIN-0152`, `LIN-1718`, `LIN-1719`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryDryRunReport {
    pub snapshot_gen: Option<u64>,
    pub head_gen: Option<u64>,
    pub repayable_frames: usize,
    pub replayable_frames: usize,
    pub final_gen: u64,
    pub truncate_to_offset: Option<u64>,
    pub damaged_offset: Option<u64>,
    pub failed_frame_hexdump: Option<String>,
    pub decision_log: Vec<String>,
}

/// Per-frame progress payload emitted during [`crate::Store::open_with_recovery_hooks`] (`LIN-0149`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryProgress {
    pub frames_replayed: usize,
    pub current_gen: u64,
    pub bytes_offset: u64,
}

/// Telemetry and size metrics returned by [`crate::Store::checkpoint_with_metrics`] (`LIN-0189`).
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointMetrics {
    pub generation: u64,
    pub wal_bytes_before: u64,
    pub forecast_snapshot_bytes: u64,
    pub duration_ms: f64,
    pub cold_tables: usize,
    pub in_memory_rows: usize,
}

/// Stage-annotated checkpoint failure provenance (`LIN-0192`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointStageError {
    pub stage: &'static str,
    pub cause: Error,
}

impl CheckpointStageError {
    pub const fn new(stage: &'static str, cause: Error) -> Self {
        Self { stage, cause }
    }

    pub fn into_error(self) -> Error {
        Error::runtime(format!("checkpoint [{}]: {}", self.stage, self.cause.message))
    }
}

impl fmt::Display for CheckpointStageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "checkpoint [{}]: {}", self.stage, self.cause.message)
    }
}

impl std::error::Error for CheckpointStageError {}

/// Live phase diagnostics for an interactive [`crate::Transaction`] (`LIN-0237`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxPhaseDiagnostics {
    pub tx_id: u64,
    pub statements_run: usize,
    pub journal_entries: usize,
    pub has_full_backup: bool,
    pub disk_savepoint: bool,
    pub elapsed_micros: u64,
}

/// Global monotonic rollback reason counters across interactive transactions (`LIN-0238`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollbackReasonCounters {
    pub explicit: u64,
    pub dropped_uncommitted: u64,
    pub statement_error: u64,
    pub commit_error: u64,
}

static ROLLBACK_EXPLICIT: AtomicU64 = AtomicU64::new(0);
static ROLLBACK_DROPPED: AtomicU64 = AtomicU64::new(0);
static ROLLBACK_STMT_ERR: AtomicU64 = AtomicU64::new(0);
static ROLLBACK_COMMIT_ERR: AtomicU64 = AtomicU64::new(0);

/// Snapshot current process-wide transaction rollback counters (`LIN-0238`).
pub fn rollback_counters() -> RollbackReasonCounters {
    RollbackReasonCounters {
        explicit: ROLLBACK_EXPLICIT.load(Ordering::Relaxed),
        dropped_uncommitted: ROLLBACK_DROPPED.load(Ordering::Relaxed),
        statement_error: ROLLBACK_STMT_ERR.load(Ordering::Relaxed),
        commit_error: ROLLBACK_COMMIT_ERR.load(Ordering::Relaxed),
    }
}

/// Reset process-wide transaction rollback counters to zero (`LIN-0238`).
pub fn reset_rollback_counters() {
    ROLLBACK_EXPLICIT.store(0, Ordering::Relaxed);
    ROLLBACK_DROPPED.store(0, Ordering::Relaxed);
    ROLLBACK_STMT_ERR.store(0, Ordering::Relaxed);
    ROLLBACK_COMMIT_ERR.store(0, Ordering::Relaxed);
}

#[inline]
pub(crate) fn inc_rollback_explicit() {
    ROLLBACK_EXPLICIT.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub(crate) fn inc_rollback_dropped() {
    ROLLBACK_DROPPED.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub(crate) fn inc_rollback_stmt_err() {
    ROLLBACK_STMT_ERR.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub(crate) fn inc_rollback_commit_err() {
    ROLLBACK_COMMIT_ERR.fetch_add(1, Ordering::Relaxed);
}

/// Format up to 64 bytes starting at `offset` (clamped within `bytes`) as lowercase space-separated hex.
pub(crate) fn format_hexdump_window(bytes: &[u8], offset: u64) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let start = usize::try_from(offset)
        .unwrap_or(bytes.len() - 1)
        .min(bytes.len() - 1);
    let end = (start + 64).min(bytes.len());
    let mut out = String::with_capacity((end - start) * 3);
    for (i, b) in bytes[start..end].iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        use fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub(crate) fn pack_kind_name(pack: &Pack) -> &'static str {
    match pack {
        Pack::Insert { .. } => "insert",
        Pack::InsertBulk { .. } => "insert_bulk",
        Pack::InsertCols { .. } => "insert_cols",
        Pack::AppendFact { .. } => "append_fact",
        Pack::AppendFactsBulk { .. } => "append_facts_bulk",
        Pack::AppendEdge { .. } => "append_edge",
        Pack::AppendEdgesBulk { .. } => "append_edges_bulk",
        Pack::Update { .. } => "update",
        Pack::Reembed => "reembed",
        Pack::Delete { .. } => "delete",
        Pack::DeleteEdge { .. } => "delete_edge",
        Pack::SchemaCol { .. } => "schema_col",
        Pack::SchemaRel { .. } => "schema_rel",
        Pack::SchemaIndex { .. } => "schema_index",
        Pack::SchemaFilter { .. } => "schema_filter",
        Pack::SchemaOwned { .. } => "schema_owned",
        Pack::RegisterSegment { .. } => "register_segment",
        Pack::Batch { .. } => "batch",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DamageClass {
    Truncated,
    Corrupt,
}

fn inspect_wal_slice(full_bytes: &[u8], start_offset: u64) -> (WalInspectionReport, Option<DamageClass>) {
    let total_len = full_bytes.len();
    let mut pos = usize::try_from(start_offset).unwrap_or(total_len).min(total_len);
    let mut frames = Vec::new();
    let mut damaged_offset = None;
    let mut damaged_reason = None;
    let mut failed_frame_hexdump = None;
    let mut damage_class = None;

    while pos < total_len {
        let off_u64 = pos as u64;
        if pos + 4 > total_len {
            let rem = total_len - pos;
            damaged_offset = Some(off_u64);
            damaged_reason = Some(format!(
                "truncated frame header at byte offset {off_u64}: {rem} of 4 magic/length bytes available"
            ));
            failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
            damage_class = Some(DamageClass::Truncated);
            break;
        }
        let head: [u8; 4] = full_bytes[pos..pos + 4].try_into().unwrap();
        if head == LOG_MAGIC_V1 || head == LOG_MAGIC_V2 || head == LOG_MAGIC_CHECKSUM {
            if pos + 8 > total_len {
                let rem = total_len - pos;
                damaged_offset = Some(off_u64);
                damaged_reason = Some(format!(
                    "truncated frame header at byte offset {off_u64}: {rem} of 8 header bytes available"
                ));
                failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                damage_class = Some(DamageClass::Truncated);
                break;
            }
            let len = u32::from_le_bytes(full_bytes[pos + 4..pos + 8].try_into().unwrap());
            let remaining_after_hdr = (total_len - (pos + 8)) as u64;
            if len == 0 || len > MAX_RECORD {
                let is_trunc = len == 0 || u64::from(len) > remaining_after_hdr;
                damaged_offset = Some(off_u64);
                damaged_reason = Some(if len == 0 {
                    format!("truncated/zero frame length (0) at byte offset {off_u64}")
                } else {
                    format!(
                        "frame length {len} exceeds MAX_RECORD ({MAX_RECORD}) at byte offset {off_u64}"
                    )
                });
                failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                damage_class = Some(if is_trunc {
                    DamageClass::Truncated
                } else {
                    DamageClass::Corrupt
                });
                break;
            }
            let need = len as usize;
            if pos + 8 + need > total_len {
                damaged_offset = Some(off_u64);
                damaged_reason = Some(format!(
                    "truncated frame payload at byte offset {off_u64}: declared {need} bytes, available {remaining_after_hdr}"
                ));
                failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                damage_class = Some(DamageClass::Truncated);
                break;
            }
            let payload = &full_bytes[pos + 8..pos + 8 + need];
            let frame_byte_len = (8 + need) as u64;

            if head == LOG_MAGIC_CHECKSUM {
                if payload.len() < 5 {
                    damaged_offset = Some(off_u64);
                    damaged_reason = Some(format!(
                        "short checksummed frame envelope ({} bytes < 5) at byte offset {off_u64}",
                        payload.len()
                    ));
                    failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                    damage_class = Some(DamageClass::Corrupt);
                    break;
                }
                let codec = payload[0];
                let body_end = payload.len() - 4;
                let expected = u32::from_le_bytes(payload[body_end..].try_into().unwrap());
                let hw_crc = crate::crc::hardware_crc32c(&payload[..body_end]);
                let crc_ok = hw_crc == expected || crc32fast::hash(&payload[..body_end]) == expected;
                if !crc_ok {
                    damaged_offset = Some(off_u64);
                    damaged_reason = Some(format!(
                        "crc mismatch at byte offset {off_u64}: stored 0x{expected:08x}, computed 0x{hw_crc:08x}"
                    ));
                    failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                    damage_class = Some(DamageClass::Corrupt);
                    break;
                }
                if !CodecCapabilities::supports_wal_codec(codec) {
                    damaged_offset = Some(off_u64);
                    damaged_reason = Some(format!(
                        "unsupported wal codec {codec} at byte offset {off_u64}"
                    ));
                    failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                    damage_class = Some(DamageClass::Corrupt);
                    break;
                }
                match decode_frame(&head, payload) {
                    Ok(rec) => {
                        frames.push(WalFrameInfo {
                            frame_index: frames.len(),
                            byte_offset: off_u64,
                            byte_len: frame_byte_len,
                            codec,
                            r#gen: rec.r#gen,
                            next_id: rec.next_id,
                            pack_kind: pack_kind_name(&rec.pack).to_string(),
                            crc_valid: true,
                        });
                        pos += 8 + need;
                    }
                    Err(e) => {
                        damaged_offset = Some(off_u64);
                        damaged_reason = Some(format!(
                            "corrupt payload at byte offset {off_u64}: {}",
                            e.message
                        ));
                        failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                        damage_class = Some(DamageClass::Corrupt);
                        break;
                    }
                }
            } else {
                let codec = head[3];
                match decode_frame(&head, payload) {
                    Ok(rec) => {
                        frames.push(WalFrameInfo {
                            frame_index: frames.len(),
                            byte_offset: off_u64,
                            byte_len: frame_byte_len,
                            codec,
                            r#gen: rec.r#gen,
                            next_id: rec.next_id,
                            pack_kind: pack_kind_name(&rec.pack).to_string(),
                            crc_valid: true,
                        });
                        pos += 8 + need;
                    }
                    Err(e) => {
                        damaged_offset = Some(off_u64);
                        damaged_reason = Some(format!(
                            "corrupt v{codec} payload at byte offset {off_u64}: {}",
                            e.message
                        ));
                        failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                        damage_class = Some(DamageClass::Corrupt);
                        break;
                    }
                }
            }
            continue;
        }

        // Legacy JSON framing: `[u32 LE len][UTF-8 JSON(LogRecord)]`
        let len = u32::from_le_bytes(head);
        let remaining = (total_len - (pos + 4)) as u64;
        if len == 0 || len > MAX_RECORD {
            let is_trunc = len == 0 || u64::from(len) > remaining;
            damaged_offset = Some(off_u64);
            damaged_reason = Some(format!(
                "invalid legacy frame header/magic at byte offset {off_u64} (declared len={len})"
            ));
            failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
            damage_class = Some(if is_trunc {
                DamageClass::Truncated
            } else {
                DamageClass::Corrupt
            });
            break;
        }
        let need = len as usize;
        if pos + 4 + need > total_len {
            damaged_offset = Some(off_u64);
            damaged_reason = Some(format!(
                "truncated legacy frame payload at byte offset {off_u64}: declared {need} bytes, available {remaining}"
            ));
            failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
            damage_class = Some(DamageClass::Truncated);
            break;
        }
        let json_slice = &full_bytes[pos + 4..pos + 4 + need];
        match serde_json::from_slice::<LogRecord>(json_slice) {
            Ok(rec) => {
                frames.push(WalFrameInfo {
                    frame_index: frames.len(),
                    byte_offset: off_u64,
                    byte_len: (4 + need) as u64,
                    codec: 0,
                    r#gen: rec.r#gen,
                    next_id: rec.next_id,
                    pack_kind: pack_kind_name(&rec.pack).to_string(),
                    crc_valid: true,
                });
                pos += 4 + need;
            }
            Err(e) => {
                damaged_offset = Some(off_u64);
                damaged_reason = Some(format!(
                    "corrupt legacy/unknown frame at byte offset {off_u64}: {e}"
                ));
                failed_frame_hexdump = Some(format_hexdump_window(full_bytes, off_u64));
                damage_class = Some(DamageClass::Corrupt);
                break;
            }
        }
    }

    (
        WalInspectionReport {
            frames,
            valid_prefix_bytes: pos as u64,
            damaged_offset,
            damaged_reason,
            failed_frame_hexdump,
        },
        damage_class,
    )
}

/// Inspect raw WAL bytes without mutating state (`LIN-0068`, `LIN-1575`, `LIN-1718`).
pub fn inspect_wal_bytes(bytes: &[u8]) -> WalInspectionReport {
    inspect_wal_slice(bytes, 0).0
}

/// Inspect a WAL file on disk without mutating or locking it (`LIN-0068`, `LIN-1575`, `LIN-1718`).
pub fn inspect_wal_file(path: &Path) -> Result<WalInspectionReport, Error> {
    let bytes = fs::read(path).map_err(io_err)?;
    Ok(inspect_wal_bytes(&bytes))
}

/// Inspect raw snapshot bytes without mutating state (`LIN-0068`).
pub fn inspect_snapshot_bytes(bytes: &[u8]) -> SnapshotInspectionReport {
    let byte_len = bytes.len() as u64;
    if bytes.is_empty() {
        return SnapshotInspectionReport {
            exists: true,
            byte_len: 0,
            format_version: None,
            crc_trailer_present: false,
            crc_valid: false,
            r#gen: None,
            collection_count: 0,
            cold_collections: Vec::new(),
        };
    }

    if bytes.len() >= 5 && bytes[1..5] == SNAPSHOT_MAGIC {
        let ver = bytes[0];
        if ver == 0x03 {
            let data = &bytes[5..];
            let mut crc_trailer_present = false;
            let mut crc_valid = false;
            let mut header_info: Option<(u64, Vec<String>, BTreeSet<String>)> = None;

            if data.len() >= 8 {
                let header_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
                if header_len <= MAX_SNAPSHOT_SECTION_BYTES && data.len() >= 4 + header_len + 4 {
                    let header_bytes = &data[4..4 + header_len];
                    let mut offset = 4 + header_len;
                    let chunk_count =
                        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
                    offset += 4;
                    let mut chunks_ok = true;
                    let mut names = BTreeSet::new();
                    for _ in 0..chunk_count {
                        if data.len() < offset + 2 {
                            chunks_ok = false;
                            break;
                        }
                        let name_len =
                            u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
                                as usize;
                        offset += 2;
                        if data.len() < offset + name_len + 4 {
                            chunks_ok = false;
                            break;
                        }
                        let Ok(name) = std::str::from_utf8(&data[offset..offset + name_len]) else {
                            chunks_ok = false;
                            break;
                        };
                        names.insert(name.to_string());
                        if names.len() > MAX_SNAPSHOT_COLLECTIONS {
                            chunks_ok = false;
                            break;
                        }
                        offset += name_len;
                        let data_len =
                            u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
                                as usize;
                        offset += 4;
                        if data_len > MAX_SNAPSHOT_SECTION_BYTES || data.len() < offset + data_len {
                            chunks_ok = false;
                            break;
                        }
                        offset += data_len;
                    }
                    if chunks_ok {
                        if data.len() - offset == 8
                            && bytes[5 + offset..5 + offset + 4] == SNAPSHOT_CRC_MAGIC
                        {
                            crc_trailer_present = true;
                            let crc_off = 5 + offset + 4;
                            let stored =
                                u32::from_le_bytes(bytes[crc_off..crc_off + 4].try_into().unwrap());
                            crc_valid = crate::crc::hardware_crc32c(&bytes[..crc_off]) == stored;
                        } else if data.len() == offset {
                            crc_trailer_present = false;
                            crc_valid = false;
                        }
                        if validate_msgpack_header_limits(header_bytes).is_ok()
                            && let Ok(hdr) = rmp_serde::from_slice::<SnapshotHeader>(header_bytes)
                            && hdr.edges.len() <= MAX_SNAPSHOT_EDGES
                            && hdr.extra_collections.len() <= MAX_SNAPSHOT_COLLECTIONS
                            && hdr.cold_collections.len() <= MAX_SNAPSHOT_COLLECTIONS
                        {
                            for k in hdr.extra_collections.keys() {
                                names.insert(k.clone());
                            }
                            for k in &hdr.cold_collections {
                                names.insert(k.clone());
                            }
                            header_info = Some((hdr.r#gen, hdr.cold_collections, names));
                        }
                    }
                }
            }

            if !crc_trailer_present && bytes.len() >= 8 {
                let tail_magic_off = bytes.len() - 8;
                if bytes[tail_magic_off..tail_magic_off + 4] == SNAPSHOT_CRC_MAGIC {
                    crc_trailer_present = true;
                    let stored = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
                    crc_valid =
                        crate::crc::hardware_crc32c(&bytes[..bytes.len() - 4]) == stored;
                }
            }

            // Verify full chunk payloads decode cleanly when trailer passed (or had no trailer).
            let decoded_ok = (!crc_trailer_present || crc_valid)
                && decode_snapshot_bytes_checked(bytes).is_ok();
            let (r_gen, cold_collections, collection_count) = match (decoded_ok, header_info) {
                (true, Some((g, cold, names))) => (Some(g), cold, names.len()),
                _ => (None, Vec::new(), 0),
            };
            return SnapshotInspectionReport {
                exists: true,
                byte_len,
                format_version: Some(0x03),
                crc_trailer_present,
                crc_valid,
                r#gen: r_gen,
                collection_count,
                cold_collections,
            };
        }

        let format_version = matches!(ver, 0x01 | 0x02 | 0x04).then_some(ver);
        if let Ok(snap) = decode_snapshot_bytes_checked(bytes) {
            return SnapshotInspectionReport {
                exists: true,
                byte_len,
                format_version,
                crc_trailer_present: false,
                crc_valid: false,
                r#gen: Some(snap.r#gen),
                collection_count: snap.collections.len(),
                cold_collections: snap.cold_collections,
            };
        }
        return SnapshotInspectionReport {
            exists: true,
            byte_len,
            format_version,
            crc_trailer_present: false,
            crc_valid: false,
            r#gen: None,
            collection_count: 0,
            cold_collections: Vec::new(),
        };
    }

    let format_version = (bytes.len() >= 4 && bytes[..4] == SNAPSHOT_MAGIC).then_some(0x00);
    match decode_snapshot_bytes_checked(bytes) {
        Ok(snap) => SnapshotInspectionReport {
            exists: true,
            byte_len,
            format_version,
            crc_trailer_present: false,
            crc_valid: false,
            r#gen: Some(snap.r#gen),
            collection_count: snap.collections.len(),
            cold_collections: snap.cold_collections,
        },
        Err(_) => SnapshotInspectionReport {
            exists: true,
            byte_len,
            format_version,
            crc_trailer_present: false,
            crc_valid: false,
            r#gen: None,
            collection_count: 0,
            cold_collections: Vec::new(),
        },
    }
}

/// Inspect a snapshot file on disk without mutating it (`LIN-0068`).
pub fn inspect_snapshot_file(path: &Path) -> Result<SnapshotInspectionReport, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(inspect_snapshot_bytes(&bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(SnapshotInspectionReport {
            exists: false,
            byte_len: 0,
            format_version: None,
            crc_trailer_present: false,
            crc_valid: false,
            r#gen: None,
            collection_count: 0,
            cold_collections: Vec::new(),
        }),
        Err(e) => Err(io_err(e)),
    }
}

/// Perform a completely non-mutating crash-recovery simulation over `head`, `snapshot`, and `log`
/// in `dir`, recording every recovery decision and any failing byte offset / frame hexdump
/// (`LIN-0151`, `LIN-0152`, `LIN-1718`, `LIN-1719`).
pub fn dry_run_recovery(dir: &Path) -> Result<RecoveryDryRunReport, Error> {
    validate_data_dir(dir)?;
    let mut decision_log = Vec::new();

    if !dir.exists() {
        decision_log.push(format!(
            "dir: '{}' does not exist; open would initialize empty store at gen=0",
            dir.display()
        ));
        return Ok(RecoveryDryRunReport {
            snapshot_gen: None,
            head_gen: None,
            repayable_frames: 0,
            replayable_frames: 0,
            final_gen: 0,
            truncate_to_offset: None,
            damaged_offset: None,
            failed_frame_hexdump: None,
            decision_log,
        });
    }

    // 1. Inspect `head` without mutating.
    let h_path = head_path(dir);
    let head_gen = match fs::read(&h_path) {
        Ok(bytes) if bytes.is_empty() => {
            decision_log.push("head: empty file (treated as absent)".to_string());
            None
        }
        Ok(bytes) => match serde_json::from_slice::<Head>(&bytes) {
            Ok(h) => {
                decision_log.push(format!(
                    "head: loaded gen={} catalog_hash={} embed_id={}",
                    h.r#gen, h.catalog_hash, h.embed_id
                ));
                Some(h.r#gen)
            }
            Err(e) => {
                decision_log.push(format!("head: corrupt JSON ({e}); recovery would fail head parse"));
                None
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            decision_log.push("head: absent (will write head after replay)".to_string());
            None
        }
        Err(e) => return Err(io_err(e)),
    };

    // 2. Inspect `snapshot` without mutating.
    let s_path = snapshot_path(dir);
    let mut snapshot_gen = None;
    let mut snap_log_offset = 0u64;
    match fs::read(&s_path) {
        Ok(bytes) if bytes.is_empty() => {
            decision_log.push("snapshot: empty file (replay starts from gen=0, offset=0)".to_string());
        }
        Ok(bytes) => match decode_snapshot_bytes_checked(&bytes) {
            Ok(snap) => {
                let s_rep = inspect_snapshot_bytes(&bytes);
                snapshot_gen = Some(snap.r#gen);
                snap_log_offset = snap.log_offset;
                decision_log.push(format!(
                    "snapshot: valid format={:?} crc_valid={} gen={} log_offset={} collections={}",
                    s_rep.format_version,
                    s_rep.crc_valid,
                    snap.r#gen,
                    snap.log_offset,
                    snap.collections.len()
                ));
            }
            Err(e) => {
                let hexdump = format_hexdump_window(&bytes, 0);
                decision_log.push(format!(
                    "snapshot: abort — corrupt or out-of-bounds snapshot ({})",
                    e.message
                ));
                return Ok(RecoveryDryRunReport {
                    snapshot_gen: None,
                    head_gen,
                    repayable_frames: 0,
                    replayable_frames: 0,
                    final_gen: 0,
                    truncate_to_offset: None,
                    damaged_offset: Some(0),
                    failed_frame_hexdump: (!hexdump.is_empty()).then_some(hexdump),
                    decision_log,
                });
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            decision_log.push("snapshot: absent (replay starts from gen=0, offset=0)".to_string());
        }
        Err(e) => return Err(io_err(e)),
    }

    if let (Some(h_gen), Some(s_gen)) = (head_gen, snapshot_gen)
        && h_gen < s_gen
    {
        decision_log.push(format!(
            "head: stale gen={h_gen} < snapshot gen={s_gen} (interrupted checkpoint; snapshot takes precedence)"
        ));
    }

    let min_gen = snapshot_gen.unwrap_or(0);

    // 3. Inspect `log` without mutating.
    let l_path = log_path(dir);
    let wal_bytes = match fs::read(&l_path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            decision_log.push(format!(
                "wal: absent; final recovered state stays at gen={min_gen}"
            ));
            return Ok(RecoveryDryRunReport {
                snapshot_gen,
                head_gen,
                repayable_frames: 0,
                replayable_frames: 0,
                final_gen: min_gen,
                truncate_to_offset: None,
                damaged_offset: None,
                failed_frame_hexdump: None,
                decision_log,
            });
        }
        Err(e) => return Err(io_err(e)),
    };

    if wal_bytes.is_empty() {
        decision_log.push(format!(
            "wal: empty (0 bytes); final recovered state at gen={min_gen}"
        ));
        return Ok(RecoveryDryRunReport {
            snapshot_gen,
            head_gen,
            repayable_frames: 0,
            replayable_frames: 0,
            final_gen: min_gen,
            truncate_to_offset: None,
            damaged_offset: None,
            failed_frame_hexdump: None,
            decision_log,
        });
    }

    let initial_start = if snap_log_offset <= wal_bytes.len() as u64 {
        snap_log_offset
    } else {
        decision_log.push(format!(
            "wal: snapshot log_offset={snap_log_offset} exceeds wal len={}; falling back to offset 0",
            wal_bytes.len()
        ));
        0
    };

    let (mut wal_rep, mut damage_class) = inspect_wal_slice(&wal_bytes, initial_start);
    if matches!(damage_class, Some(DamageClass::Corrupt))
        && wal_rep.frames.is_empty()
        && initial_start > 0
    {
        decision_log.push(format!(
            "wal: start offset {initial_start} did not align to valid frame; retrying from offset 0"
        ));
        (wal_rep, damage_class) = inspect_wal_slice(&wal_bytes, 0);
    }

    // Mirror ReplayOrder over the valid prefix frames first.
    let mut applied = min_gen;
    let mut replayable = 0usize;
    for frame in &wal_rep.frames {
        if frame.r#gen <= applied {
            decision_log.push(format!(
                "wal: skip frame #{} at offset {} (gen={} <= applied={applied})",
                frame.frame_index, frame.byte_offset, frame.r#gen
            ));
            continue;
        }
        if frame.r#gen != applied + 1 {
            let gap_hex = format_hexdump_window(&wal_bytes, frame.byte_offset);
            decision_log.push(format!(
                "wal: abort — generation gap at frame #{} (byte offset {}): expected gen {} after {applied}, found {}",
                frame.frame_index,
                frame.byte_offset,
                applied + 1,
                frame.r#gen
            ));
            return Ok(RecoveryDryRunReport {
                snapshot_gen,
                head_gen,
                repayable_frames: replayable,
                replayable_frames: replayable,
                final_gen: applied,
                truncate_to_offset: None,
                damaged_offset: Some(frame.byte_offset),
                failed_frame_hexdump: (!gap_hex.is_empty()).then_some(gap_hex),
                decision_log,
            });
        }
        applied = frame.r#gen;
        replayable += 1;
        decision_log.push(format!(
            "wal: replay frame #{} at offset {}..{} (gen={}, pack={})",
            frame.frame_index,
            frame.byte_offset,
            frame.byte_offset + frame.byte_len,
            frame.r#gen,
            frame.pack_kind
        ));
    }

    match damage_class {
        None => {
            decision_log.push(format!(
                "recovery: clean WAL ({} bytes, {replayable} frames replayed); final gen={applied}",
                wal_bytes.len()
            ));
            Ok(RecoveryDryRunReport {
                snapshot_gen,
                head_gen,
                repayable_frames: replayable,
                replayable_frames: replayable,
                final_gen: applied,
                truncate_to_offset: None,
                damaged_offset: None,
                failed_frame_hexdump: None,
                decision_log,
            })
        }
        Some(DamageClass::Truncated) => {
            let trunc_off = wal_rep.valid_prefix_bytes;
            let reason = wal_rep
                .damaged_reason
                .unwrap_or_else(|| format!("truncated trailing frame at offset {trunc_off}"));
            decision_log.push(format!("wal: {reason}"));
            decision_log.push(format!(
                "recovery: torn trailing write detected; open would truncate log from {} to {trunc_off} bytes with final gen={applied}",
                wal_bytes.len()
            ));
            Ok(RecoveryDryRunReport {
                snapshot_gen,
                head_gen,
                repayable_frames: replayable,
                replayable_frames: replayable,
                final_gen: applied,
                truncate_to_offset: Some(trunc_off),
                damaged_offset: wal_rep.damaged_offset,
                failed_frame_hexdump: wal_rep.failed_frame_hexdump,
                decision_log,
            })
        }
        Some(DamageClass::Corrupt) => {
            let reason = wal_rep
                .damaged_reason
                .unwrap_or_else(|| "corrupt wal frame".to_string());
            decision_log.push(format!(
                "wal: abort — {reason}; open refuses to truncate over corrupt frame"
            ));
            Ok(RecoveryDryRunReport {
                snapshot_gen,
                head_gen,
                repayable_frames: replayable,
                replayable_frames: replayable,
                final_gen: applied,
                truncate_to_offset: None,
                damaged_offset: wal_rep.damaged_offset,
                failed_frame_hexdump: wal_rep.failed_frame_hexdump,
                decision_log,
            })
        }
    }
}
