# Storage Guarantees, Wire/Disk Compatibility, and Sizing Guide

This document defines Lin's durability guarantees, binary file/wire format versions, downgrade rules, air-gapped deployment profile, and capacity sizing methodology (`LIN-1963`, `LIN-1969`–`LIN-1972`, `LIN-1991`, `LIN-1992`).

---

## 1. Storage & Durability Guarantee Matrix (`LIN-1963`)

| Mode / Tier | Crash Durability | Concurrency Contract | Corruption Detection |
|---|---|---|---|
| `Db::empty()` / `Db::fixture()` | In-memory only (ephemeral) | Single `&mut Db` writer + unlimited lock-free `ReadDb` clones in-process | In-memory Rust type invariants |
| `Db::open` / `OpenOpts { sync: SyncMode::Full, cold: false }` | **Strict ACID**: every acknowledged commit writes a framed WAL record and flushes drive caches (`fcntl(F_BARRIERFSYNC)` on macOS, `fdatasync` on Linux, `FlushFileBuffers` on Windows) before returning | Exclusive `LOCK` file prevents dual writers; shared `FENCE` coordinates cross-process `Db::open_read` with `checkpoint` | Per-frame CRC32C (`LOG_MAGIC_CHECKSUM`) + monotonic `ReplayOrder` generation gap detection + snapshot/FTS `LCRC` CRC32C trailers |
| `OpenOpts { sync: SyncMode::Normal, .. }` | Process-crash durable (OS page cache); power-loss durable after `Db::checkpoint()` / `Db::close()` | Same `LOCK` + `FENCE` | Same CRC32C frame & snapshot trailers; torn trailing WAL frame after power loss is truncated to last valid prefix |
| Hybrid Cold Tier (`OpenOpts { cold: true, .. }`) | **Strict ACID** (under `SyncMode::Full`): collections `>= COLD_MIN_ROWS` spill to immutable `mmap` `DiskSegment`s on checkpoint; point/bulk mutations between checkpoints log to `Persist::log` and update `MemTable`/tombstone overlays | Lock-free `Arc<DiskEngine>` reads across threads; zero cold-table promotion on point DML, composite/partial unique checks, and `Pack::RegisterSegment` batch ingest | Per-file `(byte_len, crc32c)` in `manifest.json` for all columns, hash indexes, and `tombstones.bin`; damaged optional `.zmap`/`.tgram`/`.vivf` sidecars fall back to exact scans |
| Standalone `DiskEngine` (`is_pure_disk_dir`) | Durable via `cold/wal.log` + atomic `manifest.json` + segment `sync_dir` | Single writer `LOCK` + `open_read_only` | Per-segment file length/header validation + manifest `(byte_len, crc32c)` checksums |
| Follower Hot Standby (`open_follower` / `LinServer` replica) | Durable WAL append + replay before advancing follower `gen`; refuses writes on primary WAL gaps | Read-only queries at `Consistency::{Weak, Session, AtLeastGen}` | Wire CRC32C (`LCRC`) + WAL frame CRC32C + generation continuity enforcement |

---

## 2. Binary Format & Protocol Compatibility (`LIN-1969`–`LIN-1972`)

### 2.1 WAL Format (`log` — `LIN-1969`)
- **Framing:** `[4B magic][4B len_le][payload...][optional 4B crc32c_le]`, capped at `MAX_RECORD` (`16 MiB` per frame; large cold/bulk inserts automatically split across consecutive frames in one atomic commit).
- **Codecs:**
  - `1` (`LOG_MAGIC_V1` / `LIN\x01`): MessagePack `LogRecord` (`Pack` variants). Universal baseline.
  - `2` (`LOG_MAGIC_V2` / `LIN\x02`): Binary columnar `V2_FACTS_BULK` and `V2_INSERT_COLS`. Universal across `0.5+` and `0.7.x`.
  - `3` (`LOG_MAGIC_SPARSE`): Sparse vector `V2_INSERT_COLS` (non-zero `(u16_idx, f32_bits)` pairs). Readable unconditionally on all builds.
  - `4` (Compressed WAL): Opt-in via `--features compress-wal` (`64 MiB` max decoded guard). Requires a reader compiled with the matching compression feature.
  - `5` (`LOG_MAGIC_DICTIONARY`): Opt-in via `OpenOpts::with_wal_dictionary(true)` or `persist::set_wal_dictionary_writer(true)`. Off by default so logs remain readable by releases without codec 5 (see [wal-dictionary.md](wal-dictionary.md)).

### 2.2 Snapshot & FTS Blobs (`snapshot`, `fts/*.bin` — `LIN-1970`)
- **Snapshot:** Tag `0x03` (MessagePack `Snapshot`) or `LIN\x07` (chunked Zstd columnar `LCOL` snapshot) followed by an 8-byte `LCRC` + `u32_le` CRC32C trailer.
  - *Forward/Backward Compatibility:* Older `0.7.1` readers ignore trailing bytes past the MessagePack root object, while current readers verify `LCRC` when present and accept legacy trailer-less snapshots.
- **Durable FTS (`fts/<col>.bin`):** `LIN\x05` header (`version = 2` for contextual `str::to_lowercase` Unicode tokenization) + `(generation, expected_rows, fields)` + posting lists + `LCRC` trailer. Stale or version-mismatched FTS files are automatically rebuilt from collection rows on open.
- **Cold Tier (`manifest.json`):** `DISK_FORMAT_VERSION = 3`. Readers open versions `2` and `3`; writable opens refuse `version > 3` so an older binary never silently strips newer manifest metadata.

### 2.3 Replication & RPC Wire Protocols (`LIN-1971`)
- **`ship` (`lin wal-serve` / `ship::pull`):** `LIN\x06` (pull request/response with shared token & optional `rustls` mTLS/TLS) and `LIN\x07` (`pull_wait` push-woken long-polling with `max_lag_gens` cutoff). `Pack::RegisterSegment` frames are transparently expanded into portable `Pack::InsertCols` frames when exported over the wire.
- **`server` (`lin serve` / `lin client`):** `LINS\x01` + `LCRC` framed persistent keep-alive RPC; distributed sharding adds `LAGG\x01` partial `summarize` states.

### 2.4 Downgrade Support Boundaries (`LIN-1972`)
- **Supported without migration (`v0.7.x <-> HEAD`):** Databases using default WAL codecs (`1..=3`), `DISK_FORMAT_VERSION <= 3` cold segments (including `.zmap`, `.tgram`, `.vivf` sidecars and `file_crcs` in `manifest.json`), and `LCRC` trailers are bidirectionally verified on every CI run by [`./scripts/format-compat.sh`](../scripts/format-compat.sh).
- **Requires action before downgrade:**
  1. If `wal_dictionary` (codec 5) or `compress-wal` (codec 4) was enabled, run `db.checkpoint()` (or `lin checkpoint --data <dir>`) so `log` is compacted to `0` bytes before opening with an older binary.
  2. Never force-open a directory whose `manifest.json` reports a higher `DISK_FORMAT_VERSION` in read-write mode; use logical backup (`lin backup export` / JSONL export) when crossing major format boundaries.

---

## 3. Air-Gapped Deployment (`LIN-1991`)

Lin runs completely offline by default:
1. **Zero network egress in default build:** `HashingEmbedder` computes deterministic trigram/bigram embeddings locally in CPU registers; `Db`, `DiskEngine`, `LinServer` (loopback/LAN), and `FsObjectStore` make zero external HTTP calls.
2. **Offline build preparation:**
   ```sh
   cargo vendor --locked vendor/
   # Place [source.crates-io] replace-with = "vendored-sources" in .cargo/config.toml
   cargo build --release --offline --locked --bin lin
   ```
3. **Local neural embeddings (optional):** Build with `--features embed-onnx` and stage the `.onnx` model + `tokenizer.json` on local disk; enforce `security::CapabilityPolicy { allow_network_models: false, .. }` to forbid remote embedding providers (`embed-ollama`).

---

## 4. Resource Sizing Methodology (`LIN-1992`)

Estimate steady-state and peak resources from collection cardinality $N$, average scalar/text payload $B_{\text{row}}$, and vector dimension $D$:

- **In-Memory Hot Tier RAM:**
  $$\text{RAM}_{\text{hot}} \approx N \times \left(B_{\text{row}} + 48\text{ B (Row + by\_id)}\right) + N \times 4D\text{ B (f32 vec)} + \text{Indexes/FTS}$$
  - When `FlatVecMatrix` is active on a searched vector collection, add $N \times (D + 4)\text{ B}$ (`~7.7 MiB` per `10k × 768` vectors).
  - Secondary non-unique `LiveIndex` uses `BitSet` (`N / 8` bytes per distinct low-cardinality key) or `SmallVec` (`8 B` per row index).
- **Cold Tier (`DiskEngine`) RAM & Disk:**
  - Resident RAM is independent of total disk size (`~5–20 MiB` metadata + configurable `HotCacheConfig`, default `64 MiB`), while OS page cache maps active `.bdat`/`.val`/`.vsq8` pages on demand.
  - Disk footprint: Zstd text blocks typically compress prose `2.5x–4x`; vectors store `4D` bytes (`.vf32`) + `D + 8` bytes (`.vsq8` quantized prefilter).
- **WAL & Checkpoint Headroom:**
  - Reserve at least $2 \times \text{Quotas::max\_log\_bytes}$ plus `1.2x` the largest single cold segment or snapshot file on the target filesystem so atomic `.tmp` write + `fsync` + `rename` never hits `ENOSPC` mid-checkpoint.
