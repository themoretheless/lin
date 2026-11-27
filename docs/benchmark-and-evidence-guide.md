# Benchmark Interpretation, Profiling Cookbook, ADRs, and Roadmap Governance

This document provides the benchmark reading guide, matched-peer rules, profiler recipes, Architecture Decision Records (ADRs), rejected-experiment registry, and evidence governance for Lin (`LIN-1639`, `LIN-1973`–`LIN-1976`, `LIN-1990`, `LIN-1993`–`LIN-2000`).

---

## 1. Benchmark Interpretation & Matched-Peer Guide (`LIN-1973`–`LIN-1976`, `LIN-1990`)

### 1.1 Reading Measurements Correctly (`LIN-1973`)
- **Batch average vs tail latency:** `benches/compare.rs` measures calibrated batches (`operations` per sample) and summarizes **per-process medians** first (`docs/benchmark-contract.md`). Batch `ns/op` is amortized throughput latency, not single-request P99 tail latency under cold caches.
- **Durability parity on macOS (`Darwin`) vs Linux:**
  - On macOS, `Lin (SyncMode::Full)` issues `fcntl(fd, F_BARRIERFSYNC)` (~`450–600 µs` physical NVMe barrier). Upstream SQLite's default `PRAGMA synchronous = FULL` (`fullfsync = OFF`) calls plain `fsync(2)` without a drive cache flush; always compare `SyncMode::Full` against `sqlite_fullfsync` (`PRAGMA fullfsync = ON`) for barrier parity, and `SyncMode::Normal` against `sqlite_wal_normal`.
- **Clock & thermal drift:** Laptop sleep/Low-Power states scale Apple Silicon cores by `~1.5x`. Always run full sweeps under `caffeinate -i` and cross-check unchanged in-process anchors (`compare/point_get/hashmap` ~ `6.6–7.0 ns`, `sqlite` point get ~ `250 ns`).

### 1.2 Matched-Peer Fixture Contract (`LIN-1974`)
Every peer case in [`benches/compare.rs`](../benches/compare.rs) and [`scripts/bench-peers.py`](../scripts/bench-peers.py) enforces:
1. Identical row counts (`N = 10,000` warm reads, `1k`/`10k`/`100k`/`1M` DML, `10,000 orders ⋈ 1,000 users` joins) and identical column payloads.
2. Prepared statement parity (`Db::prepare` once outside timing vs `sqlite::Connection::prepare_cached` / DuckDB prepared / `Appender`).
3. Post-timing readback validation (`bench_checked` with `DropPolicy::OutsideTiming`) verifying row counts and sampled cell values.
4. No disabling of FTS, auto-embedding (`HashingEmbedder`), or secondary indexes on Lin's full ingest cases (`insert_phase_10k/*` breaks out phase costs separately for attribution only).

### 1.3 Raw Artifact Navigation & Current Frontier (`LIN-1975`, `LIN-1976`)
- **CI Gate Output:** `./scripts/ci-bench.sh` writes `bench-report.md` and `.airbug-bench/ci/run.json` (schema-validated JSON containing `environment`, `provenance`, `cases`, and raw per-sample `observations`).
- **Historical & Acceptance Baselines:** Stored under `benches/results/` (e.g., `benches/results/2026-11-20-roadmap-acceptance/report.md`) and `docs/benchmark-history.md`.
- **Current Performance Snapshot (macOS `aarch64`, `compare --profile quick`):**
  - Hot reads: `point_get` `13.6 ns` (`18.6x` vs SQLite), `filter_eq` `218 ns` (`311x` vs SQLite, `313x` vs DuckDB), `fts_lex_common` `1.16 µs`, `fts_hybrid_common` `197 µs`, `graph_depth_1..3` `96–192 ns`, `reader_snapshot` `148 ns`, `agg_sum_by` `20.6 µs` (`18x` vs DuckDB), `join_inner` `128 µs` (`7.8x` vs DuckDB).
  - Cold mmap tier: `cold_point_get` `72.8 ns` (`48.5x` vs SQLite), `cold_filter_range` `2.58 µs` (`27.6x` vs SQLite, `42.3x` vs DuckDB), `cold_join_inner` `120.9 µs` (`8.8x` vs DuckDB), `cold_delete_1row_10k` `502 µs` (`29.6x` vs `sqlite_fullfsync`, `1.48x` vs DuckDB).
  - Remaining external peer audit: live Azure Data Explorer / Kusto endpoint (`LIN-0006`, `blocked_external`) and physical Linux `ext4`/`xfs` fixed-length WAL comparison (`P4.2`).

### 1.4 Dedicated Benchmark Server Setup (`LIN-1990`)
- Disable turbo/power-saving jitter (`cpupower frequency-set -g performance` on Linux; AC power + `caffeinate -i` on macOS).
- Place `/tmp` / benchmark scratch directories on local NVMe (`ext4`/`xfs` or APFS), never network mounts.
- Pin Dockerized peers (`docker compose -f docker-compose.bench.yml up -d postgres mysql`) on isolated cores or measure in alternating process-pairs (`>= 6` pairs per series).

---

## 2. Profiler Command Cookbook (`LIN-1639`)

```sh
# 1. Build bench binary with frame pointers / debug symbols:
CARGO_PROFILE_BENCH_DEBUG=true cargo bench --bench compare --no-run

# 2. macOS CPU sampling (`sample` or Instruments `xctrace`):
xcrun xctrace record --template 'Time Profiler' --output /tmp/lin.trace \
  --launch -- ./target/release/deps/compare-* --profile quick --filter insert_bulk_10k

# 3. Linux hardware PMU counters & flamegraph (`perf`):
perf stat -e cycles,instructions,cache-misses,branch-misses \
  ./target/release/deps/compare-* --profile quick --filter durable_insert_10k
perf record -F 999 --call-graph dwarf -- ./target/release/deps/compare-* --profile quick --filter join_inner

# 4. Built-in phase attribution without external profilers:
cargo bench --bench compare -- --profile quick --filter phase
```

---

## 3. Architecture Decision Records (`LIN-1999`)

- **ADR-001 — Slotted `Row` + `InlineStr` SSO (`src/store/row.rs`, `src/store/value.rs`):** Replaced `BTreeMap<String, Cell>` with `Arc<RowShape>` + contiguous `Cell` slice and 22-byte inline `SmallText`, cutting per-row heap allocations from `O(cols)` to `1` and making `Shape::position` a branchless `u16`-tag scan.
- **ADR-002 — `ExactSum` (`i128` + Neumaier) Across All Backends (`src/exact_sum.rs`):** Unified scalar, grouped, parallel-chunked (`16,384`-row chunks), spilled, `ColMirror`, `Column::sum_f64`, cold-tier, and GPU sums on exact `i128` integer + compensated `f64` float accumulation so results never drift past $2^{53}$ or across thread counts.
- **ADR-003 — Copy-on-Write Snapshots & Lock-Free `Arc<DiskEngine>` (`src/store/mod.rs`, `src/index.rs`, `src/fts.rs`):** Wrapped `RowMap`, `LiveIndex::forward`, `FtsIndex`, and clean `DiskEngine` views in `Arc` (`Arc::make_mut` on write), making `Db::reader()` `O(1)` (~`148 ns`) and removing `RwLock` from all cold reads.
- **ADR-004 — Single-WAL Hybrid Cold Overlay (`src/disk/mod.rs`, `src/exec/mod.rs`):** Separated `*_mem_only` overlay updates (durable via `Persist::log`) from standalone `*_mem_and_wal` updates, eliminating double WAL writes and double `F_BARRIERFSYNC` on cold 1-row DML.
- **ADR-005 — Universal Typed `ColMirror` & Vectorized `RecordBatch` (`src/exec/mirror.rs`, `src/batch.rs`):** Extended lazy epoch-validated `ColMirror` with dense `Int`/`Float`/`Time`/`Bool`/`Text` + `Dict` columns for arbitrary user `Catalog` collections, feeding native vectorized `RecordBatch` filter/project/without/sort/agg operators.
- **ADR-006 — 64-Shard `CacheNode` with Intrusive `Slab`-LRU (`src/cache/store.rs`):** Replaced single-lock `BTreeSet` LRU with 64 cache-padded `RwLock<CacheShard>` stripes and an `O(1)` doubly-linked `Slab` with `AtomicBool` second-chance read hits.

---

## 4. Rejected-Experiment Registry (`LIN-1998`)

1. **Unconditional `Cell::from_arc_str` (`SmallText` conversion) inside `orders`/`users`/`docs` SoA join/agg outputs:** Rejected because stored `orders`/`users` rows keep `Cell::Text(Arc<str>)`, and `Debug`-format parity tests (`tests/persist.rs`) assert identical variant representation between row and columnar pipelines.
2. **Unbounded stack hash-interning on all `V2_FACTS_BULK` columns:** Hashing 100% unique payload columns (`s`, `o`) added `~17 µs` per 1k facts with `0%` hit rate; replaced with an adaptive 32-probe hit check that disables slot hashing on unique columns while keeping 1-instruction `last_raw` run-length sharing.
3. **Always-on `O(N log N)` pair sorting in `LiveIndex::insert_slab_batch`:** Caused a `4x` regression (`124 µs -> 488 µs`) on low-cardinality composite keys (`docs[wing, ts]`, $K = 2$); replaced with MRU 4-slot cache + `FxHashMap` key aggregation (`O(N + K log K)` -> `63.7 µs`).
4. **Character-by-character `char::to_lowercase` in GPU lexical search:** Broke contextual Greek final sigma (`ΟΔΟΣ` -> `οδοσ` vs `οδος`); replaced with `str::to_lowercase`.

---

## 5. Roadmap Triage Policy & Completion Checklist (`LIN-1993`–`LIN-1997`, `LIN-2000`)

- **Triage Order (`LIN-1997`):** (1) Reproducible data loss or silent wrong answer (`G-CORRECT`) → (2) Measured peer loss or regression under matched contract (`G-PERF`) → (3) Security/resource-limit boundary (`G-VERIFY`/`G-OPS`) → (4) User-facing API/CLI/feature (`G-FEATURE`/`G-API`/`G-DOC`).
- **Evidence Linkage & Completion Checklist (`LIN-1993`–`LIN-1996`, `LIN-2000`):**
  - Every closed `G-CORRECT` / `G-VERIFY` item links to an automated test in `tests/*_qualification.rs` or unit test module that fails if the invariant is broken.
  - Every closed `G-PERF` item links to raw `run.json` / `benches/compare.rs` measurements under matched durability and readback validation.
  - Every closed `G-FEATURE` / `G-API` / `G-OPS` / `G-DOC` item compiles cleanly under `--no-default-features`, default, and `--all-features` with `0` warnings and is documented in `CHANGELOG.md` and `docs/`.
  - Items requiring external infrastructure (`LIN-0006` live Kusto endpoint, `P4.2` physical Linux `ext4`/`xfs`, Windows/Vulkan/DX12 hardware) remain explicitly open in `docs/roadmap.md` until executed on target hardware.
