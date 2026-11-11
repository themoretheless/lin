//! On-disk layout of a durable Lin data dir (default `./.lin`):
//!
//! ```text
//! <data>/
//!   head       JSON object: gen, catalog_hash, embed_id
//!   log        append-only checksummed records (`LIN\x06`; reads v1/v2)
//!   snapshot   `LIN\x04` + MessagePack(Snapshot); legacy JSON still reads
//!   fts/       posting lists per collection (`LIN\x05`), written on checkpoint
//!   cold/      optional mmap cache of large collections (same rows also inlined
//!              in snapshot — self-contained)
//! ```
//!
//! Log record framing (v1 binary, dual-read with legacy JSON):
//! - **v1:** magic `LIN\x01` + `u32` LE payload length + MessagePack(`LogRecord`)
//! - **v2:** magic `LIN\x02` + raw columnar hot packs
//! - **v3:** magic `LIN\x06` + `u32` LE envelope length + codec byte (1/2/3)
//!   + encoded payload + CRC32 LE of codec/payload; all new writes use v3
//! - **legacy:** `u32` LE length + JSON(`LogRecord`) — still replayed on open
//!
//! A truncated trailing record is ignored. After replay the log is truncated
//! to the last complete record. Checkpoint compacts the log to empty.
//! Portable backup = self-contained `LIN\x04` MessagePack (never cold stubs).
//! FTS blobs are local accel; missing/stale files rebuild on open.


mod codec;
mod snapshot;
mod types;
mod wal;

#[cfg(test)]
mod tests;

pub use codec::{cols_to_rows, rows_to_insert_cols};
pub(crate) use codec::{
    PreparedEncoding, PreparedInsert, acquire_encoder, append_prepared_record,
    encode_insert_rows_direct, prepare_insert_columns_ext, start_insert_encoding,
};

pub use snapshot::{
    encode_snapshot_ref_to_writer, read_backup, read_snapshot, write_backup, write_snapshot,
    write_snapshot_ref, write_snapshot_ref_to_file,
};
pub(crate) use snapshot::write_snapshot_ref_to_file_ext;
#[cfg(test)]
pub use snapshot::encode_snapshot_ref_bytes;
#[cfg(all(
    test,
    any(
        feature = "compress-flate",
        feature = "compress-lz4",
        feature = "compress-zstd"
    )
))]
pub use snapshot::encode_snapshot_compressed;

pub use types::{
    BACKUP_MAGIC, ColData, ColSnap, FENCE_NAME, HEAD_NAME, Head, IndexSnap, LOCK_NAME, LOG_NAME,
    LogRecord, OpenMemOpts, Pack, Persist, RelSnap, SNAPSHOT_EVERY, SNAPSHOT_MAGIC, SNAPSHOT_NAME,
    Snapshot, SnapshotRef, SyncMode,
};

pub use wal::{
    acquire_fence_exclusive, acquire_reader_lock, acquire_writer_lock, append_raw_frames,
    append_record, compact_log, ensure_dir, export_wal_since, export_wal_since_indexed, fence_path,
    for_each_wal_frame, for_each_wal_frame_raw, head_path, io_err, lock_path, log_len, log_path,
    open_log, open_log_read, read_head, replay_log, snapshot_path, truncate_log, validate_data_dir,
    write_head, write_head_soft, write_through_tmp,
};
#[cfg(all(test, feature = "parallel"))]
pub use wal::append_batch_records;
pub(crate) use wal::{
    append_validated_frame_slices, append_validated_frames, durable_sync, prune_bins,
    replay_log_indexed, sync_dir_checked, write_frame, write_head_no_dir_sync,
};
