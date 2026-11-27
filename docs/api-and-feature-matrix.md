# API Stability, Feature Matrix, and Execution Semantics

This document specifies the stability tiers, Cargo feature flags, memory ownership, concurrency, cursor snapshot pinning, and cancellation boundaries of the `lin` crate (`v0.7.1`, [src/lib.rs](../src/lib.rs)).

---

## 1. Public API Stability Matrix (`LIN-1961`)

| Surface | Entry Points | Tier | Semver Policy |
|---|---|---|---|
| Core Database Lifecycle | `Db::{empty, fixture, open, open_with, open_read, open_follower, bootstrap_follower, close, checkpoint, reader, stats, with_quotas, with_sync_mode, with_embedder}` | **Stable** (`0.3+` freeze) | Breaking signature or semantic changes require a major version bump post-`1.0`. |
| Query Execution & Prepared Plans | `Db::{run, execute, scalar, run_group, prepare, prepare_query, explain_as}`, `ReadDb::{run, prepare, stats, clone}`, `Prepared`, `Handle`, `Done` | **Stable** (`0.3+` freeze) | Preserved across minor releases; new methods added compatibly. |
| Fluent & Typed Row API | `Queryable`, `BoundQueryable`, `query::pred`, `QueryCursor`, `ProjectedRow`, `FromRow`, `LinRow`, `FromCell`, `ToCell`, `Col`, `map_rows`, `cell_get`, `cell_opt` | **Stable** (`0.3+` freeze) | Derived and manual `FromRow` implementations remain source-compatible. |
| Async Offload Façade | `AsyncDb`, `AsyncReadDb`, `Queryable::to_vec_async`, `stream_*` (`feature = "async"`) | **Stable** (`0.3+` freeze) | Backed by `tokio::task::spawn_blocking` and shared `WalSyncGroup` group commit. |
| Core Value & Schema Types | `Row`, `RowShape`, `Cell`, `InlineStr`, `Store`, `Plan`, `Stats`, `ReopenPhases`, `Quotas`, `SyncMode`, `OpenOpts`, `Embedder`, `HashingEmbedder`, `Error` | **Stable** (`0.3+` freeze) | `Row` preserves map-like lookup (`get`, `insert`, `iter`); `Cell` variants are documented below. |
| Columnar OLAP & Batch Execution | `RecordBatch`, `Column`, `Validity`, `Db::run_batch`, `Db::insert_batch`, `Db::declare_columns`, `Db::enable_catalog_columns` | **Experimental** | Wire tags (`1..=7`) are covered by unit/integration roundtrips; Rust struct internals may evolve in minor releases. |
| Cold Tier & Object Storage | `DiskEngine`, `ColdObjectStore`, `FsObjectStore`, `S3HttpObjectStore`, `MemoryObjectStore`, `RemoteCacheConfig` | **Experimental** | On-disk segment format (`DISK_FORMAT_VERSION = 3`) is gated by `scripts/format-compat.sh`. |
| Networked Server, Replication & Sharding | `LinServer`, `ServerClient`, `ReplicaPoolClient`, `ShardRouter`, `ShardedCluster`, `ship::*`, `cache::*` (`cacha`) | **Experimental** | Wire protocols (`LINS\x01`, `LIN\x06`/`LIN\x07`, `LINC`) carry explicit magic bytes and CRC32C trailers. |
| Diagnostics, Telemetry, Cost Stats, I/O & Migration | `error::*`, `telemetry::*`, `security::*`, `stats::*`, `io_formats::*`, `migration::*`, `persist::inspect::*` | **Experimental** | Additive introspection and operational tooling; safe no-op when unconfigured. |

---

## 2. Cargo Feature Support Matrix (`LIN-1962`, `LIN-1989`)

| Feature Flag | Default | External System / Crate Requirements | Scope & Behavior |
|---|---|---|---|
| `default` | **Yes** | Rust stable toolchain (`edition = "2024"`) | Enables `derive` and `async`. Zero external C/C++ runtime required for `lin` library or `lin` CLI (`libduckdb-sys` and `rusqlite` are `dev-dependencies` only). |
| `derive` | **Yes** | `lin-derive` workspace proc-macro | Exports `#[derive(LinRow)]` for typed projection, optional fields (`Option<T>`), `#[lin(rename = "...")]`, and `#[lin(owned)]` nested structs. |
| `async` | **Yes** | `tokio` (`rt-multi-thread`, `sync`, `macros`, `net`, `time`) | Enables `AsyncDb`, `AsyncReadDb`, async query streams, and `SharedDb` group-commit transaction coalescing. |
| `parallel` | No | `rayon` | Enables multi-core chunked OLAP scans/aggregations (`>= 65,536` rows), parallel HNSW/embedding batches, and index bloom filters. Numerical sums match single-threaded chunked `ExactSum` bit-for-bit. |
| `gpu` | No | `wgpu`, `bytemuck`, `pollster` + physical Metal / Vulkan / DX12 adapter | Hardware WGSL compute kernels for scalar filters, UTF-8/regex matching, exact binary64 sums, joins, stable sort, cosine/RRF search, and BFS/DFS graph walks. Refuses software fallback adapters; see [compute-support.md](compute-support.md). |
| `embed-ollama` | No | `ureq` + reachable Ollama HTTP endpoint | Enables `OllamaEmbedder` (`ollama:<model>@<dim>`). Keep disabled or pair with `security::CapabilityPolicy` in air-gapped deployments. |
| `embed-onnx` | No | `ort` (ONNX Runtime) + `ndarray` + local `.onnx` / `tokenizer.json` files | Enables local neural `OnnxEmbedder` without network egress. |
| `compress-zstd` | No | `zstd` (note: core `lin` also links `zstd` for cold text blocks & `LIN\x07` snapshots) | Enables Zstd frame support on optional compressed paths. |
| `compress-lz4` | No | `lz4_flex` (pure Rust) | Enables LZ4 WAL/snapshot frame codec support. |
| `compress-flate` | No | `flate2` | Enables Deflate compression codec support. |
| `compress-wal` | No | Implies `compress-zstd` + `compress-lz4` | Enables codec-4 compressed WAL frame writing (bounded to `64 MiB` decoded on replay). |

Verification across feature combinations:
```sh
cargo check --all-targets
cargo check --no-default-features --all-targets
cargo check --all-targets --all-features
```

---

## 3. Memory Ownership Model (`LIN-1981`)

1. **Slotted `Row` & Interned `RowShape` ([src/store/row.rs](../src/store/row.rs))**:
   - Every stored `Row` holds an `Arc<RowShape>` (sorted column names + `u16` `(len << 8 | first_byte)` SIMD-friendly lookup tags) and a contiguous `Box<[Cell]>` (or small inline storage). Cloning a `Row` clones `Arc<RowShape>` in `O(1)` and clones each `Cell`.
2. **`Cell` String & Vector Ownership ([src/store/value.rs](../src/store/value.rs))**:
   - `Cell::SmallText(InlineStr)` stores UTF-8 strings up to 22 bytes inline on the stack (`Copy`, zero heap allocation, branchless fixed-width equality).
   - `Cell::Text(Arc<str>)` and `Cell::Vec(Arc<[f32]>)` are immutable reference-counted slices shared zero-copy across `Store`, `ReadDb` snapshots, `ColMirror`, `RecordBatch`, and `Handle` results.
3. **Copy-on-Write Snapshots ([src/store/mod.rs](../src/store/mod.rs))**:
   - `RowMap`, `LiveIndex::forward`, `FtsIndex`, and clean read-only `Arc<DiskEngine>` tiers use `Arc::make_mut` Copy-on-Write. Calling `Db::reader()` shares immutable backing structures in `O(1)` (~`150 ns`) without copying row vectors, B-Trees, or FTS postings; subsequent writer mutations detach only touched structures.
4. **Thread-Local Row Recycling**:
   - Projected 2-column `{id, title}` rows recycle shapes/allocations through bounded thread-local pools (`ID_TITLE_POOL`) when `Arc::strong_count == 1`, never retaining unbounded heap memory across drops.

---

## 4. Async Threading, Snapshot Pinning & Cancellation (`LIN-1982`–`LIN-1984`)

- **Async Thread Model (`LIN-1983`)**:
  - `AsyncDb` and `AsyncReadDb` ([src/async_db.rs](../src/async_db.rs)) do **not** perform asynchronous kernel storage I/O (`io_uring`); CPU-bound query evaluation and `F_BARRIERFSYNC`/`fdatasync` commits are offloaded onto Tokio's `spawn_blocking` pool.
  - Concurrent `AsyncDb::run` and `AsyncDb::transaction` writers automatically coalesce into shared `WalSyncGroup` batches so `N` concurrent callers amortize a single WAL barrier sync.
  - `AsyncDb::stream` evaluates chunks on a blocking worker and sends batches over a bounded `mpsc` channel (`64..512` rows), providing automatic backpressure when the async consumer is slow.
- **Cursor & `ReadDb` Snapshot Semantics (`LIN-1984`)**:
  - `QueryCursor` borrows `&Db` or `&ReadDb` for its lifetime (`'a`), seeing a frozen, repeatable-read point-in-time view at the snapshot's generation `gen`.
  - Dropping a `QueryCursor` early immediately releases any scratch buffers or cold segment block caches without scanning remaining rows.
  - Across processes, `Db::open_read` acquires a shared `FENCE` lock while `Db::checkpoint` acquires an exclusive `FENCE` lock before compacting/rewriting snapshots, guaranteeing active cross-process readers never observe torn files.
- **Cancellation & Deadlines (`LIN-1982`)**:
  - Queries respect `Quotas` (`max_scan_rows`, `max_output_bytes`, `max_aggregate_bytes`, `max_graph_expansion`), server `WorkloadGroup` timeouts, and recovery `is_cancelled` hooks. Aborted reads leave database state untouched; aborted multi-statement programs or interactive `Transaction`s roll back atomically via reverse undo journals or pre-statement backups before returning `Error`.
