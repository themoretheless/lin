# Changelog

## Unreleased

### Fixed
- Deleting a row that a foreign key still references is now refused (`fk: … still references …`) on every delete path, in memory and on the cold tier. Before, the delete went through and left dangling references whose rows then vanished from `join`. Delete or repoint the referencing rows first; a self-referencing row may be deleted together with everything that references it. This changes behaviour for code that deleted referenced rows.
- With a GPU attached, `sum` / `sum by` no longer differ from the CPU: the GPU summed integers as f64 (rounding past 2^53) and floats without the CPU's compensation. Integers are now summed exactly on the host, and the GPU sum kernel keeps the same Neumaier compensation as the CPU, so results match bit for bit. `GpuCompute::group_sums` still returns the plain left-to-right sum.
- Lexical search lowercased the index character by character but the query with the contextual Unicode rules, so an upper-case Greek word never found its lower-case form: `ΟΔΟΣ` became `οδοσ` in the index, while the stored `οδος` ends in `ς`. Both sides now use `str::to_lowercase`. The full-text index format moves to version 2, so existing index files are rebuilt on open. The built-in embedder uses the same lowering, so vectors of texts with a word-final capital sigma change.
- `sum` and `avg` over floats lost small values between large values that cancel: `1e16, 1, -1e16` summed to `0`. In-memory, grouped, parallel, spilled, indexed and join aggregates now use Neumaier-compensated addition and give `1`. The cold tier's columnar float sum is compensated the same way.
- Sliding-frame window `sum` and `avg` over floats subtracted running prefix sums, so small values vanished behind a large earlier value: after `1e16`, a frame of `1.0, 0.5, 1.0` summed to `0`. Frames up to 1024 rows that do not start at the first row are now summed from their own rows.
- `lin cache cli` with commands piped in mixed its banner and `cacha (...)>` prompts into the output. The banner and prompts are now shown only when stdin is a terminal.
- The `lin` binary panicked with `failed printing to stdout: Broken pipe` (exit code 101) when its output went to a pipe that closed early, such as `lin run ... | head`. It now stops quietly on SIGPIPE like other command-line tools.
- Error messages quoted user input in full: a statement made of a one-megabyte identifier produced a one-megabyte `unknown collection` error. Messages are now cut at 4 KiB on a character boundary, with a note of how many bytes were dropped.
- `#[derive(LinRow)]` failed with `missing column` for an `Option<T>` field whose value was never set: rows are sparse, so such a field is absent rather than `Null`. An absent field now reads as `None`; required fields still report the missing column.
- Sorting compared integers through f64: integers above 2^53 that differ by one tied or swapped (`2^53 + 1` sorted before `2^53`, `i64::MAX` before `i64::MAX - 1`), with or without an index. Integers now compare exactly with each other and with floats.
- String literals dropped the backslash of any escape other than `\n \t \\ \"`: `regex "\d+"` matched `d+`, and `"\r"` became `r`. Literals now also accept `\r \0 \' \u{..}` (the form string values are printed in, so printed text parses back unchanged), and any other escape keeps its backslash. A malformed `\u{..}` is an error.
- `sum` and grouped `sum` over integers added the values as floats, so totals lost precision once they passed 2^53: `i64::MAX + i64::MIN` came back as `0` instead of `-1`. Integers are now added exactly in 128 bits (in memory, in parallel chunks, when spilled, through an index and on the cold tier) and rounded to a float only at the end.
- `append facts` with more than 128 rows in one statement reported the rows as written while the next query saw none of them; after a restart, facts replayed from the log were invisible and a fact deleted before the restart came back after the next write. Fact rows are now kept in step with their columns on every append.
- A database with a `col` / `index` statement in the log and no checkpoint after it refused to open (`catalog_hash mismatch`). Opening now accepts every catalog state the log replay goes through; a different caller catalog is still rejected.
- A compressed WAL frame (codec 4, features `compress-wal` / `compress-flate`) was decompressed without a size limit; it is now refused past 64 MiB decoded.
- A vectored WAL write interrupted by a signal failed the commit instead of being retried.
- `update` could rename a row onto an `id` another row already held, or give several rows one `id`, leaving duplicate primary keys. Both are now rejected with `duplicate id`.
- After `update` changed a row's `id` in memory, a point lookup by the old `id` still returned the row and the new `id` was not found (the id map was rebuilt from the row before it was written). The same applied to rolling back such an update.
- `docs | uri == "..." | pick { ... }` on a cold collection returned an overlay row with all its fields instead of the projection.
- The `gpu` feature did not compile after the `Row` change (`gpu_sum_rows` and `tests/gpu.rs` still built `BTreeMap` rows).
- Opening a database replayed log frames in file order without checking their generations: a frame present twice applied its rows twice (two documents with one `uri`), a frame out of order moved the generation backwards, and a missing frame was skipped silently. A repeated frame is now skipped; a log with a hole or frames out of order is refused (`corrupt log: generation gap`) and left unmodified, as a follower already did.
- A crash during a checkpoint after the new snapshot was in place but before `head` was rewritten left a database that refused to open (`catalog_hash mismatch`) when the snapshot included a schema change: the stale `head` still carried the old catalog hash. A `head` older than the snapshot is no longer checked against it; the snapshot carries its own hash.
- An index range or equality-prefix lookup on integers beyond f64 precision (above 2^53 in magnitude) returned neighbouring values: `a <= -9223372036854775808` also matched `-9223372036854775807`, because the bounds were widened to the whole f64 bucket while row comparison is exact. Bounds now use the literal's exact key.
- A multi-row `insert` whose first record had an explicit `id` left the later records that had none without any id, so they could not be looked up, updated or deleted by key. Every record without an `id` now gets a generated one, wherever it sits in the batch.
- A query with a long flat chain of `or` / `and` terms (around 100 000) parsed, then crashed the whole process with a stack overflow while being checked and evaluated: the depth limit counted parentheses and `not` but not chained operators, which nested one level per term. Chains of 32 or more terms are now joined as a balanced tree, so even a million terms nest about twenty levels deep; the AST size quota still rejects the largest ones.
- `ship::pull` allocated the payload size the server announced (up to 256 MiB) before receiving any of it; it now grows with the data received and reports a payload shorter than announced as truncated.
- A window `sum` over integers near the i64 limit wrapped around (two `i64::MAX` summed to `-2`) because prefix sums were kept in i64. They are now kept in i128; a frame sum outside i64 is returned as a float, like the table-level `sum`.
- Bulk inserts (`insert_batch`, large `insert` statements) split full-text fields on ASCII whitespace only, while single-row inserts, removals and queries split on Unicode whitespace: words separated by a vertical tab, a no-break space or another Unicode space were not searchable, and removing such a document left a stale combined token in the index. All paths now split the same way. Indexes already on disk keep the old tokens until they are rebuilt.
- The bulk full-text path took the positions of the indexed fields from the first row of a batch and applied them to every row with the same number of fields, so a row with a different set of fields had the wrong field indexed and its own title or body missing from the index. Rows are now checked for identical fields.
- `update` kept a document's old embedding when its `title`, `body` or `snippet` changed, so vector and hybrid search ranked it by text it no longer had. A changed text field is now embedded again (in memory, on the cold tier and on the point-update path) unless the statement sets `embedding` itself.
- A durable database used with `Db::with_embedder` could not be reopened (`catalog_hash mismatch`): `head` recorded the new embedder id next to the catalog hash computed before the embedder was set. The cached hash now follows the embedder, and heads written that way still open.
- A cold-tier database whose `manifest.json` was missing opened with its cold collections empty, and the next checkpoint made that permanent. Opening now fails when the snapshot lists cold collections that the disk tier does not have, and the disk engine refuses to start an empty manifest over existing segments.
- A truncated or empty cold column file read as missing values for the rows past its end. Segment columns are now checked against their row count when opened (row index, block offsets and the last stored block for text, codes for dictionaries, values and null flags for fixed-width columns).
- A failed write could empty a cold collection on disk: a multi-statement program or transaction that inserted into a cold collection and then failed (for example on a duplicate key in another collection) truncated the collection to its in-memory length, zero, and the next checkpoint wrote that. Writes to cold collections are now undone from a full backup that re-attaches the untouched disk table.
- Interactive transaction rollback could lose or keep rows: a `delete` followed by an `insert` dropped the row the delete had moved into the gap, and once the journal switched to a full backup part way through (a schema statement, or an insert after updates or deletes), the writes made before the switch survived `rollback`. The journal now records every append point and keeps the earlier journal under a mid-transaction backup, so rollback undoes statements in exact reverse.
- Directory sync on Windows failed with `Access is denied` (`File::open` on a directory handle) when writing cold segments or snapshots; directory sync is now Unix-only while file syncs remain intact on all platforms.
- Snapshot (`0x03`) and durable FTS posting (`fts/<collection>.bin`) files had no whole-file checksum, so a corrupted or partially truncated file could panic on decoded capacity headers, serve stale postings, or open with altered snapshot data when the log was empty after a checkpoint. Both formats now append an 8-byte `LCRC` + CRC32C trailer verified on open (with bounded capacity preallocation and posting bounds checks on FTS) while remaining readable by `0.7.1` and continuing to accept earlier trailer-less files.
- Batch `orders` / `users` SoA joins now raise the parallel worker spawn threshold to `PAR_AGG_MIN_ROWS` (avoiding OS thread-spawn and cross-core refcount contention on small batches), fill text columns directly, and extend typed `RecordBatch` columns by slice instead of converting to `Vec<Cell>`.
- `delete` dropped rows that another row still referenced through a declared `fk`, leaving dangling keys that vanished from `join`. Deleting a referenced row is now refused (in memory and on the cold tier); self-referencing rows deleted in the same statement are still allowed.
- Cold-tier segment files and `tombstones.bin` now record `(byte_len, crc32c)` checksums in `manifest.json` without bumping `DISK_FORMAT_VERSION` (remaining bidirectionally compatible with `0.7.1`): silent byte corruption in column data, primary/secondary hash tables, or tombstones is rejected on open, while damaged optional pruning sidecars (`.zmap`, `.tgram`, `.vivf`) are discarded so queries fall back to exact column scans (`tests/cold_corruption_qualification.rs`: 0 silent wrong answers across all 108 damaged copies).
- Swapping models via `Db::with_embedder` on a non-empty vector collection no longer overwrites `store.embed_id` prematurely: `search vec` and `search hybrid` refuse mismatched models until `Db::reembed_collection` recomputes vectors in parallel and durably logs `Pack::Update` + `Head`.
- `Store::orders_soa_ready` and `Store::users_soa_ready` no longer materialize cold disk-backed tables into `LazyRows` when checking SoA readiness, keeping cold joins (`cold_join_inner`) on the zero-hydration `DiskEngine` path.
- `avg` and `avg by` over integers exceeding `2^53` now divide from exact 128-bit sums rather than rounding individual terms through `f64`, `Column::sum_f64` accumulates integers in `i128` and floats with Neumaier compensation via `ExactSum`, GPU lexical search (`gpu_lex_scores`) uses contextual `str::to_lowercase` matching CPU and FTS (preserving Greek word-final sigma `ς`), deferred document deletes remain invisible across index and point lookups before FTS flush, `CacheNode` RMW commands advance `mutation_order` monotonically, and `lin backup import --data <dir>` now refuses to clobber a non-empty directory unless `--force` (`-f`) is passed (`Db::import_backup_into_checked`).

### Added
- `wal-serve --sinks N` / `ship::serve_blocking_fanout`: up to 64 followers pull at once, so a stalled one does not hold up the rest.
- Cold tier format version 3 in the manifest: a writable open refuses a newer format. `scripts/format-compat.sh` and the `format-compat` workflow check that the current tree and the previous release read each other's databases.
- `sort` / `skip` / `take N` after an aggregate are answered by the aggregate plan; ungrouped `avg` / `min` / `max` run in parallel under the `parallel` feature.
- Cold tier scans: predicates are compiled per subtree (only leaves the columnar kernels cannot decide fall back to row evaluation), zone maps prune under `not` and `!=`, index lookups with a residual predicate decode output columns only for matching rows, and joins on a non-`id` key read only the key column.
- The columnar window path accepts a `sort` between the windows and the projection.
- Cold tier: zone maps also prune blocks for `is null` / `is not null` on fixed-width columns; a row join on `id` against a cold collection looks every distinct key up in one batch ordered by position on disk (`DiskEngine::get_many_by_id`).
- Cold collections stay on disk for more writes: composite and partial unique indexes (text, numeric, boolean, time and missing key values) are checked against the disk engine, `insert ... with edge` works inside programs and transactions, a declared vector index no longer forces promotion, an `update` that changes `id` is logged as a delete plus an upsert, and one insert may carry up to 1,048,576 rows.
- Cursors over a cold collection stream it from disk in chunks, joins included (left side streamed, right side read by `id`), and vector search cursors use the out-of-core search; row joins probe a cold right side instead of materializing it.
- `AsyncDb::transaction` shares commit groups with statements (one sync per group).
- `insert_batch` into a cold collection builds rows straight from the columns (no per-row statement record), about 1.5x faster on 60,000 rows; it is no longer capped by the parser's AST size quota.
- `persist::set_wal_dictionary_writer`: opt-in writer for the dictionary vector WAL codec (codec 5). Off by default; a release without the codec cannot read such frames.
- `Db::declare_columns`: opt-in, lazily built dictionary-coded mirrors (`ColMirror` over `RowMap` write epochs) for chosen fields of in-memory collections, accelerating filters, projections, sorts, windows, FK joins, and `count` / `count by` without adding write-path overhead; shared across `ReadDb` snapshots and automatically invalidated on writes or rollbacks.
- Multi-metric and multi-key `summarize { ... } by ..., bin(...)` (`count`, `sum`, `avg`, `min`, `max`, `dcount` exact-to-HLL, `percentile` exact-to-t-digest, and scalar `arg_max` / `arg_min`), whole-row `arg_max` / `arg_min` pipeline steps, excluding projection `without { ... }` (preserving row sparsity and skipping excluded columns on cold scans), cost-ordered `and` / `or` predicate flattening across hot and cold tiers, and matching fluent `Queryable` / `BoundQueryable` builders.
- Cold-tier block pruning and segment reclamation: per-block dictionary zone maps (`DictZoneMap` over `==`, `!=`, `in`, `~`, `has`, `is null`, and `not`), optional 4096-bit trigram Bloom filter sidecars (`col_<name>.tgram`) skipping Zstd block decompression on `~`, `has`, and `has_ci`, and O(1) dropping of 100%-dead segments on checkpoint and `vacuum` (deferred while a savepoint is active).
- Networked TCP/TLS RPC server & client (`src/server.rs` + `lin serve` / `lin client`): persistent `LINS\x01` + `LCRC` keep-alive transport, primary group-commit batching bounded by `group_commit_window` / `max_group_size` / `max_group_bytes` into a single WAL `fsync`, lock-free cached `ReadDb` reader pool, `Consistency::{Weak, Session, AtLeastGen, Strong}`, `WorkloadGroup` byte/row/timeout governance, and automatic follower WAL/cold-bootstrap replication (`ReplicaPoolClient`, `FollowerSyncOpts`).
- Distributed horizontal sharding (`src/cluster.rs` + `lin shard`): `ShardStrategy::{JumpConsistentHash, HashMod}` with canonical value hashing, `ShardConfig` / `ShardRouter` / `ShardedCluster`, predicate-based shard pruning, parallel DML/DDL and columnar `Column::gather` `insert_batch` routing, and two-phase distributed query plans (`LAGG\x01` `summarize` partials including HLL `dcount`, t-digest `percentile`, and `arg_max`/`arg_min`, legacy `count`/`sum`/`avg`/`min`/`max`, two-stage whole-row `ArgExtremum`, and distributed `sort | skip | take | window` with top-k pushdown).
- Cold-tier Object Storage offloading (`src/disk/remote.rs` + `lin remote`): `ColdObjectStore` (`MemoryObjectStore`, `FsObjectStore`, and AWS SigV4 HTTP/1.1 + `rustls` `S3HttpObjectStore`), `.lseg` CRC32C segment bundles, `LRMF\x01` manifests, lazy 1-GET remote open (`RemoteCacheConfig`), on-demand collection hydration (`run_remote`, `hydrate_cold_collection`), and whole-table LRU local cache eviction (`evict_cold_cache_to`).
- Unlimited chunked cold-tier inserts & columnar batch ingest (`P1.0`, `P4.1`, `P4.3`): cold `insert` and `insert_batch` split payloads exceeding `131,072` rows or `4 MiB` into consecutive WAL frames under a single group-synced commit without promoting cold tables into RAM, `insert_batch` builds shaped rows with inline content hashes and parallel `HashingEmbedder` vectors in one pass (and writes segments directly via `DiskEngine::write_bulk_segment_batch` on native disk), and WAL codec 5 encodes dictionary vectors in a single pass.
- Extended columnar windows, cold lazy cursors, and hot-cache index seeks (`P1.2`, `P2.1`, `P3.2`, `P3.3`): `try_window_project_batch` handles unprojected windows (in memory and over uniform non-null cold tables) and multi-key post-window `sort` chains; `QueryCursor` lazily streams cold `search vec|lex|hybrid`, depth-1 `hop`, and `hot ⋈ cold` `LazyJoinCursor` queries without hydrating cold tables; `DiskEngine::get_many_projected_by_id` batches sorted cold dimension lookups for `join_disk_batch`; `DiskEngine::index_eq_late` caches repeat residual queries in `HotTierCache`; and `RowProjector` eliminates per-row key allocations on projections.
- Low-latency push WAL replication & lag governance (`P4.4`, `src/server.rs`): `LinServer` wakes long-polling followers via `gen_cv` on commit, exports WAL tails from `wal_index` byte windows without holding the primary writer mutex, exposes connected follower lag via `ServerHandle::replica_statuses` (`ReplicaStatus`), and enforces `ServerHandle::set_max_replica_lag_gens` automatic bootstrap cutoffs.
- Zero-promote cold DML edge cases, Zstd block readahead, end-to-end field slots, cold non-`id` / multi-hop cursors, grouped/sparse columnar windows, and persisted direct segment ingest (`P1.0`, `P1.3`, `P2.1`, `P3.2`, `P3.3`, `P4.1`): `DiskUniqueKey` handles `NaN` float keys and `docs` without `uri` without promoting cold collections; `MmapTextCol` issues bounded `MADV_WILLNEED` readahead on sequential scans; `OwnedFieldSlot`, `OwnedPredSlots`, and `OwnedRowProjector` remove per-row field binary searches in `Prepared` (`PointGetPlan`, `AggPlan`), `LazyCursor`, and batch FK checks (`check_batch_fks`); `DiskJoinCursor` and `LazyJoinCursor` stream cold non-`id` FK joins and `try_lazy_hop` walks depths `1..=3` without hydrating cold tables; `try_window_project_batch` / `try_window_sparse_rows` execute post-window filters, `without`, grouped `count`/`sum`/`avg`/`min`/`max by`, and unprojected windows over mixed-sparsity collections; and `insert_batch` under an active `Persist` WAL writes immutable `DiskSegment`s directly via `Pack::RegisterSegment` (re-expanding into `Pack::InsertCols` on remote WAL export).
- Dense `Column::Text` / `Column::Dict` in `RecordBatch`, dynamic `Catalog` SoA mirrors, pipelined `SharedDb` group-commit transactions, Linux WAL `FALLOC_FL_KEEP_SIZE` preallocation, per-instance `OpenOpts::with_wal_dictionary`, and live reader-mode `wal-serve` (`P2.2`, `P4.2`, `P4.3`, `P4.4`): `RecordBatch` now supports packed UTF-8 `Column::Text` (wire tag `6`) and dictionary-coded `Column::Dict` (wire tag `7`) across OLAP filters, joins, sorts, windows, aggregates, and shard gather; `orders.total` preserves `Int` vs `Float` via `orders_total_tag` without reading `Row`; `Db::enable_catalog_columns` enables lazy columnar `ColMirror` projection for arbitrary `Catalog` schemas; `SharedDb` and `SharedTransaction` coalesce concurrent interactive ACID transactions (`shared.transaction()? ... tx.commit()?`) and single statements into shared `SyncMode::Full` WAL barriers while `reserve_wal_space` preallocates 1 MiB physical chunks on Linux via `fallocate(FALLOC_FL_KEEP_SIZE)`; `OpenOpts::with_wal_dictionary` / `Db::with_wal_dictionary` isolate WAL codec-5 writing per database instance (verified under 40+ mid-frame crash truncation offsets); and `ship::serve_dir_live` (`lin wal-serve`) serves WAL tails without holding the writer lock, with `LIN\x07` push long-polling (`ship::pull_wait`) and `ShipWalHub` `max_lag_gens` enforcement.
- Incremental materialized `summarize` views, time-range interval overlap indexes, bit-identical chunked `ExactSum`, `catalog_hash` repair, and `Embedder::dim()` policy (`P1.2`, `P2.3`, `P3.1`, `LIN-0001`, `LIN-0002`, `LIN-0013`, `LIN-0101`, `LIN-0504`): `Db::create_materialized_view` / `query_materialized_view` maintain partial `summarize` accumulators (`count`, `sum`, `avg`, `min`, `max`, `dcount`, `percentile`, `arg_max`, `arg_min`, `bin(...)`) incrementally in `O(Δrows)` across pure appends (`RowMap::rewrite_epoch`) and automatically answer matching `summarize` queries; `Db::create_interval_index` / `query_interval_overlap` build a 64-row block-partitioned sorted-start interval index augmented with a `max(end)` segment tree and `O(1)` amortized append tail, accelerating `start <=/< q_end and end >=/> q_start` queries in `O(log B + K)`; single-threaded `agg_sum_filtered` and `agg_avg_filtered` now fold `>= 65,536` rows in `PAR_AGG_CHUNK_ROWS` (`16,384`) chunks matching `--features parallel` bit-for-bit; `Db::repair_catalog_hash` (`lin repair-catalog-hash` / `lin recover --fix-catalog-hash`) migrates stale `head` / `snapshot` catalog hashes; `Db::with_strict_embed_dim` enforces optional strict `Embedder::dim()` length checks while keeping mixed-dimension corpora valid by default; `Persist::encode_buf` shrinks back `<= 1 MiB` after oversized commits; and acceptance benchmarks (`benches/results/2026-11-20-roadmap-acceptance/report.md`) verify `2.5x–2.6x` native 1k and `1.68x–1.75x` `SyncMode::Full` durable 1k wins over SQLite across two 6-pair process series plus wins across all 11 `cold_*` cases.
- Engine-wide hot/cold/OLAP/search/graph/cache optimization suite: compiled `PointGraphPlan` (`graph_depth_1/2/3` up to `37x` faster), `ScanProjectPlan` + `O(1)` Copy-on-Write `RowMap` / `LiveIndex` / `FtsIndex` / `Arc<DiskEngine>` snapshots (`reader_snapshot` `43.5x` faster, lock-free cold reads without `RwLock`), `O(K)` single-term FTS streaming (`fts_lex_common` `78.7x` faster), flat `SQ8` (`vdotq_s32`) + exact `f32` `FlatVecMatrix` vector search (`fts_hybrid_common` `4.9x` faster), MRU slab non-unique index builds (`O(N + K log K)`, `rebuild_scalar_indexes` `1.95x` faster than baseline) and `O(1)` `BitSet` cardinality tracking, single-`fsync` cold overlay DML (`cold_delete_1row_10k` `502 µs`, `cold_update_1row_10k` single WAL barrier), cold dictionary zone-map pruning + dense SIMD scans (`cold_filter_range` `3.25x` faster, `cold_agg_count_by` `2.54x` faster, `cold_join_inner` `1.72x` faster), universal typed `ColMirror` (`DenseInt`/`DenseFloat`/`DenseTime`/`DenseBool`/`DenseText`/`Dict`) for user `Catalog` collections with in-place `ExactSum` aggregation, end-to-end vectorized `RecordBatch` `Filter`/`Project`/`Without`/`Sort`/`Skip`/`Take` pipelines, 64-row `LazyJoinSoa` cursor batching (`join_filter/lin_cursor` `1.36x` faster), stack-scoped `work_steal` lanes, and 64-shard `CacheNode` keyspace striping with an intrusive `O(1)` doubly-linked `Slab`-LRU.

### Changed
- `lin::Row` is a slotted row (`src/store/row.rs`) instead of `BTreeMap<String, Cell>`. The map-like API is kept; code using `BTreeMap`-specific calls needs adjusting. New: `Row::from_parts`, `Row::from_sorted`, `Row::shape_of`, `store::row::FieldSlot`.

## [0.5.0] — 2026-11-11

### Added

- **Durable FTS:** checkpoint writes `fts/<collection>.bin` (`LIN\x05` posting lists).
  Open loads matching gen/nrows/fields and incrementally applies the WAL tail;
  missing or stale blobs still rebuild.
- **Ship:** protocol `LIN\x06` + shared token; optional rustls (`--tls-cert` /
  `--tls-key` / `--tls-ca`); one live sink. Loopback plaintext remains the local recipe.
- **Distributed cache & Raft cluster (`lin::cache`):** binary protocol (`LINC`),
  in-memory sharded cache store, Raft consensus node, cluster client/server, and
  consistent hashing ring.
- **Hardware GPU compute (`feature = "gpu"`):** `wgpu` compute kernels for vector
  cosine scoring, aggregate scans, and regex DFA matching (`src/gpu.rs`, `src/gpu_regex.rs`).
- **Storage & WAL compression:** opt-in `compress-zstd`, `compress-lz4`, `compress-flate`,
  `compress-wal`, chunked parallel snapshot codec (`LIN\x07`), and in-memory dictionary
  columns + bitmap indexes for low-cardinality fields.
- **Vector indexing:** HNSW with SQ8 scalar quantization (`VectorIndex`).
- **Out-of-core execution:** spill-to-disk aggregation fallback under tight memory quotas
  and cost-based adaptive hash join strategy.
- `.filter(|u| u.coin.gt(300))` on `from_typed` (closure → `Pred`).
  `.filter("coin > 300")` / `p!(coin > 300)` on untyped `from`.
- Dapper/ADO verbs: `Db::execute` (`n`+`gen`), `Db::scalar` / `Queryable::scalar`,
  `first` / `first_or` / `single` (explicit `take 1`/`2`, not implicit 50),
  `BoundQueryable::cursor`, `buffered` alias for `to_vec`.
  Typed: `first_typed` / `first_or_typed` / `single_typed`, `QueryCursor::next_typed`,
  `Handle::{first,first_typed,rows_typed}`. `ToCell`/`FromCell`
  for `Option`, `Arc<str>`, `Vec<f32>` / `Arc<[f32]>`; `cell_opt`.
- `#[lin(rename = "users.email")]` now parses (`owned` no longer dropped the `rename` value).
- Compact `ProjectedRow` cursor output via `QueryCursor::next_projected`.
- Catalog `filter` / `unfilter` (read-path global pred) and `docs all` / `Queryable::ignore_filters`.
- `Queryable::prepare` / `Db::prepare_query` cache Query AST → `Prepared` (no typecheck on hit).
- `owned` value objects flatten onto a collection; `ToCell` + `#[lin(owned)]` nested `FromRow`.
- `Db::run_group` atomic group commit and real-open phase metrics in `Stats::reopen`.
- **Window analytical & aggregate functions:** `window <func> [<field>] [by <cols>] [sort <cols> [desc|asc]] [rows [between] <start> and <end>] as <ident>`.
  Full support for `row_number`, `rank`, `dense_rank`, `lead`, `lag`, `first_value`, `last_value`, `sum`, `avg`, `min`, `max`, and `count`.
  Multi-column partitioning (`by [col1, col2]`) and multi-column ordering (`sort [col1 desc, col2 asc]`).
  Window framing: `rows between <start> and <end>` and `rows <n> preceding` for running totals, moving averages, rolling bounds.
  Allows downstream filtering (`| rnk <= 3`) and projections without subqueries.
  Fluent methods on `Queryable` and `BoundQueryable`: `.window()`, `.window_with_frame()`, `.row_number_by()`, `.rank_by()`, `.dense_rank_by()`, `.running_sum()`, `.running_avg()`.
- **Full-Text Search (FTS) wildcard prefix matching:** `search lex "prefix*"` / `search "prefix*"` merges vocabulary posting lists and scores word prefixes accurately.
- **Extended aggregations:** `sum <col>` (total without `by`), `min <col> [by <grp>]`, `max <col> [by <grp>]`, `avg <col> [by <grp>]`.
- **New query predicates:** `not <pred>` / `!<pred>`, `<col> is null`, `<col> is not null`, `<col> in [v1, v2, ...]`, and case-insensitive token search `<col> has_ci "needle"`.
- **Expanded numeric conversion:** `FromCell` and `ToCell` implementations for `i32`, `i16`, `i8`, `u32`, `u16`, `u8`, `usize`, `f32`.
- Batch embedder hook; hashing embed reuses lowercase scratch across slabs.

### Fixed

- **Writer lock inheritance:** release writer `flock` in the owning process before inherited descriptors close across forked children.
- **Partial index membership & uniqueness:** release partial-index membership and unique keys when updated rows leave the predicate; preserve `pred_src` in `Pack::SchemaIndex` and checkpoint `IndexSnap`.
- **Explicit unlimited `take`:** honor explicit unlimited `take` in source scan prefetch.
- **Cold storage lookup:** point/index lookups now read through `store.collection()` supporting `ColdCol` mmap storage.
- **Transaction rollback:** `facts` rollback restored for materialized rows and `facts_by_spo` index maps on batch abort.
- **Collection counts:** fixed double-decrement of `col_counts` on delete.
- **Facts append performance:** eliminated $O(N^2)$ regression on single fact row appends.
- **Cursor multi-conjunct filters:** preserved unindexed conjuncts and all DNF branches when reading via partial index.

### Performance

- **Morsel-driven CPU parallelism:** parallelized columnar joins, scans, grouped aggregations, 100k bulk inserts, and vector cosine scoring.
- **Point DML & interactive transactions:** fast-path point deletes/updates, zero-copy row deletion, and zero-alloc FTS pending edits.
- **Graph adjacency index:** added `edges_out` and `edges_in` in `Store` for $O(1)$ neighbor lookup during `hop`, `graph`, and `match`.
- **PK join cursor fast path:** joins on primary key (`to_field == "id"`) use $O(1)$ point lookup (`get_by_id`) instead of constructing a hash table across the entire right collection.
- **Top-K partial sorting:** `sort | take K` uses `select_nth_unstable_by` instead of sorting all rows.
- FTS lex `take` uses borrowed `(score, row_idx)` top-k instead of cloning and
  fully sorting all candidate rows.
- Reopen no longer builds row maps and scalar indexes twice around WAL replay; eliminated redundant reopen `fsync`s.
- Profile-backed `#[inline]` and bounds-check removal in the aligned SoA cursor loop; thin LTO (`codegen-units = 1`) enabled for release and bench profiles.

## [0.4.0] — 2026-09-16

### Added

- **m2 FTS:** in-memory postings (`src/fts.rs`) for catalog fields with `fts: true`
  (`docs.title` / `docs.body`). Rebuild-on-open / after writes (no durable FTS blob).
  `search lex` and hybrid lex-half use postings → residual `lex_score`. Plan/explain
  label **`FtsSeek`** (not `IndexSeek`).
- **m5 lazy cursor:** `hop` depth=1 and `search lex|hybrid | take` are lazy
  (`QueryCursor::is_lazy()`), driven by FTS/neighbor idxs.

### Docs

- README roadmap: 0.4 done; guarantees table lists FTS; lazy cursor shapes updated.

## [0.3.1] — 2026-09-16

### Added

- **m1 (opt-in):** feature `embed-ollama` → [`OllamaEmbedder`] (HTTP `/api/embeddings`);
  feature `embed-onnx` → [`OnnxEmbedder`] (fastembed / ONNX Runtime). Same [`Embedder`]
  trait; wire with `Db::with_embedder`. **Default remains [`HashingEmbedder`]**.
- **m4:** `run_batch` columnar path for `filter? | project | skip* | take?`
  (`try_filter_project_batch`) without row-map materialization; join path unchanged.

### Performance

- Join SoA (`orders ⋈ users`): probe by user index (no per-user `Vec<Cell>`); unrolled
  hot project `{ id, users.email, total }` (~24% faster `join_inner/lin` on quick profile).
- Lazy join cursor: SoA path for the same hot shape (`LazyJoinSoa`) — ~3.0 ms → ~1.7 ms
  on `join_inner/lin_cursor`.
- Materialize: compact `{ id, title }` row builder (`row_id_title`).

### Honesty

Neural backends are feature-gated and never the default. `search` / hybrid without
`with_embedder` still use local hashing vec. ONNX first use may download model/
runtime weights.

## [0.3.0] — 2026-09-15

### Stable API freeze

`Queryable` / `BoundQueryable` / `query::pred`, `QueryCursor`, `FromRow` / `LinRow` /
`FromCell` / `map_rows` / `cell_get`, and feature `async` (`AsyncDb` / `AsyncReadDb`,
`to_vec_async` / `stream_*`) move into the **0.3 stable** contract (see crate docs and
README «Стабильный API»).

`async` remains **default-on**. Streams offload work with `spawn_blocking` — this is
not async storage I/O.

Also stable: `Db::with_embedder` / `HashingEmbedder` / `Embedder` (search honesty
unchanged: default hybrid = lex + hashing vec).

### Added

- Fluent `Queryable::search_vec` / `BoundQueryable::search_vec` (parity with DSL
  `search vec`).

### Still experimental

- `ship` (plaintext TCP WAL pull/serve; no TLS/auth/fanout)
- `RecordBatch` / wider OLAP `run_batch`
- `Db::run_stmt`, `RowExt`
- `Stmt` / plan IR internals

### Migration from 0.2.x

1. Bump dependency: `lin = "0.3"` (and `lin-derive` if used directly).
2. No required code changes if you already used Queryable/cursor/async — behavior is
   unchanged; the surface is now covered by the stable list.
3. Prefer fluent `search_vec("…")` instead of only string DSL for vec ranking.
4. Continue to treat `ship` and `RecordBatch` as unstable across minors.

## [0.2.0] — prior

Durable single-writer, backup/FENCE/cold/SyncMode, follower `apply_wal` / bootstrap,
hashing vec + hybrid RRF, Queryable/cursor/async (experimental), TCP `wal-serve` /
`follower sync`, vec WAL v2, soft bench budgets.
