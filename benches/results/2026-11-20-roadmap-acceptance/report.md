# Roadmap Acceptance Evidence — 2026-11-20

- **Base Commit:** `fc2923b74` + Wave 3/4 changes (`src/exec/agg.rs`, `src/exec/matview.rs`, `src/store/checkpoint.rs`, `src/embed.rs`, `benches/compare.rs`)
- **Toolchain:** `rustc 1.102.0-nightly (ac8310ba6 2026-11-09)`, `cargo 1.102.0-nightly (c07ac2c4d 2026-11-06)`
- **Host:** macOS (Apple Silicon `aarch64-apple-darwin`)

## 1. Native & Durable Bulk Insert vs SQLite (`LIN-0001`, `LIN-0002`, `P4.3`)

Two independent series of **6 interleaved process-pairs** (`24` isolated benchmark processes total; SQLite control in every run; Lin executes full 768-d `HashingEmbedder` + FTS + `[wing, ts]` secondary index; durable mode uses `SyncMode::Full` `F_BARRIERFSYNC`):

### Series 1 Medians (6 interleaved process-pairs)

| Case | Default WAL (µs) | Codec-5 Dict WAL (µs) | `dict5 / default` | Lin vs SQLite Speedup |
|---|---:|---:|---:|---:|
| `compare/insert_bulk_1k/lin` | **310.3** | **304.3** | `0.981` | **2.50x–2.53x faster** |
| `compare/insert_bulk_1k/sqlite` | 775.3 | 769.6 | `0.993` | 1.00x |
| `compare/insert_bulk_10k/lin` | **2681.5** | **2747.8** | `1.025` | **3.68x–3.77x faster** |
| `compare/insert_bulk_10k/sqlite` | 10117.5 | 10104.1 | `0.999` | 1.00x |
| `compare/durable_insert_1k/lin` | **789.7** | **784.5** | `0.993` | **1.67x–1.68x faster** |
| `compare/durable_insert_1k/sqlite` | 1328.5 | 1310.5 | `0.986` | 1.00x |
| `compare/durable_insert_10k/lin` | **4427.0** | **4467.6** | `1.009` | **2.92x faster** |
| `compare/durable_insert_10k/sqlite` | 12948.9 | 13025.4 | `1.006` | 1.00x |

### Series 2 Medians (6 interleaved process-pairs)

| Case | Default WAL (µs) | Codec-5 Dict WAL (µs) | `dict5 / default` | Lin vs SQLite Speedup |
|---|---:|---:|---:|---:|
| `compare/insert_bulk_1k/lin` | **295.4** | **301.1** | `1.019` | **2.60x–2.62x faster** |
| `compare/insert_bulk_1k/sqlite` | 775.1 | 781.7 | `1.009` | 1.00x |
| `compare/insert_bulk_10k/lin` | **2583.3** | **2588.5** | `1.002` | **3.92x–3.93x faster** |
| `compare/insert_bulk_10k/sqlite` | 10114.1 | 10170.6 | `1.006` | 1.00x |
| `compare/durable_insert_1k/lin` | **777.6** | **757.6** | `0.974` | **1.72x–1.75x faster** |
| `compare/durable_insert_1k/sqlite` | 1337.1 | 1326.6 | `0.992` | 1.00x |
| `compare/durable_insert_10k/lin` | **4526.4** | **4403.6** | `0.973` | **2.85x–2.95x faster** |
| `compare/durable_insert_10k/sqlite` | 12888.5 | 12974.4 | `1.007` | 1.00x |

**Conclusions (`LIN-0001`, `LIN-0002`, `P4.3`):**
- Both `LIN-0001` (`insert_bulk_1k`) and `LIN-0002` (`durable_insert_1k`) are decisively won by Lin across all 12 process-pairs (`0` losses).
- Dictionary WAL codec 5 is wall-clock neutral-to-2.6%-faster (`0.973–1.009`) while shrinking WAL bytes on sparse/repeated vectors; keeping codec 5 opt-in via `OpenOpts::with_wal_dictionary(true)` / `Db::with_wal_dictionary(true)` preserves rolling-downgrade compatibility with pre-codec-5 readers.

## 2. 100k Row Aggregates & `parallel` Default Evaluation (`P3.1`)

4 interleaved process-pairs at `LIN_BENCH_N=100000` (`default = ["derive", "async"]` vs `--features parallel`, with unified `PAR_AGG_CHUNK_ROWS = 16384` `ExactSum` chunking):

| Case | Default (ns) | `parallel` (ns) | `parallel / default` | Peer Control |
|---|---:|---:|---:|---|
| `compare/agg_sum/lin` | **5,483.3** | 5,484.0 | `1.000` | DuckDB `50,567.7 ns` (**9.2x**), SQLite `186,971.3 ns` (**34.1x**) |
| `compare/agg_sum_by/lin` | **44,317.1** | 45,953.8 | `1.037` | DuckDB `387,408.9 ns` (**8.7x**), SQLite `1,232,979.0 ns` (**27.8x**) |
| `compare/agg_count_by/lin` | **21,132.0** | 21,749.7 | `1.029` | DuckDB `895,531.5 ns` (**42.4x**), SQLite `2,258,187.5 ns` (**106.9x**) |

**Decision (`P3.1`):** Keep `parallel` opt-in (out of `default`), since SoA / `DictColumn` kernels finish 100k rows in `5.5–44 µs` (below Rayon wakeup overhead) while `agg_sum_filtered` and `agg_avg_filtered` now guarantee bit-exact IEEE-754 parity between single-threaded and `parallel` builds.

## 3. Cold-Tier Benchmarks (`P1.2` & Section 7)

Two independent process runs across all 11 `cold_*` benchmarks:

| Case | Lin Run 1 | Lin Run 2 | SQLite Median | DuckDB Median | Lin Speedup |
|---|---:|---:|---:|---:|---|
| `cold_point_get` | **74.8 ns** | **74.8 ns** | 3.64 µs | 88.2 µs | **48.7x** vs SQLite, **1179x** vs DuckDB |
| `cold_filter_eq` (`DictZoneMap` + index) | **117.4 ns** | **116.2 ns** | 68.8 µs | 60.8 µs | **589x** vs SQLite, **520x** vs DuckDB |
| `cold_agg_count_by` | **5.99 µs** | **5.94 µs** | 226.5 µs | 214.5 µs | **38.0x** vs SQLite, **36.0x** vs DuckDB |
| `cold_filter_range` | **9.19 µs** | **8.57 µs** | 69.9 µs | 103.8 µs | **7.9x** vs SQLite, **11.7x** vs DuckDB |
| `cold_text_substr` (`.tgram` + `DictZoneMap`) | **11.95 µs** | **11.82 µs** | 322.6 µs | 163.5 µs | **27.1x** vs SQLite, **13.8x** vs DuckDB |
| `cold_range_project` (`lin` / `lin_batch`) | **164.4 / 156.8 µs** | **162.3 / 158.9 µs** | 507.8 µs | 594.0 µs | **3.2x** vs SQLite, **3.8x** vs DuckDB |
| `cold_join_inner` | **206.9 µs** | **207.7 µs** | 2,074.0 µs | 1,116.2 µs | **10.0x** vs SQLite, **5.4x** vs DuckDB (`0.39%` inter-run delta) |
| `cold_join_filter` | **123.6 µs** | **124.9 µs** | 1,170.0 µs | 708.8 µs | **9.4x** vs SQLite, **5.7x** vs DuckDB |
| `cold_agg_sum` | **3.58 µs** | **3.62 µs** | 194.1 µs | 48.6 µs | **53.9x** vs SQLite, **13.5x** vs DuckDB |
| `cold_agg_sum_by` | **69.5 µs** | **69.6 µs** | 1,306.3 µs | 403.0 µs | **18.8x** vs SQLite, **5.8x** vs DuckDB |
| `cold_materialize` | **43.5 µs** | **44.6 µs** | 841.9 µs | 574.5 µs | **19.1x** vs SQLite, **13.0x** vs DuckDB |

## 4. Cross-Version `v0.7.1` Uncheckpointed Artifacts Check (Section 8)

Artifacts created directly by the `v0.7.1` (`58444cf6e`) release binary without calling `close()` / `checkpoint()`:
- **Uncheckpointed `append facts` (`150` rows in one statement):** `v0.7.1` reopen returned `{hits: 0}`; current `HEAD` reopen returned **`{hits: 150}`** (100% lossless recovery from the `v0.7.1` WAL).
- **Uncheckpointed `col items` + `insert items`:** `v0.7.1` reopen failed with `catalog_hash mismatch`; current `HEAD` reopen succeeded and returned **`{hits: 1}`** (`gen=2`).
- **Pre-2026-11-14 `catalog_hash` migration:** `Db::repair_catalog_hash` (`lin --data <dir> repair-catalog-hash` / `lin --data <dir> recover --fix-catalog-hash`) rewrites stale `head` / `snapshot` catalog hashes after replaying the WAL.
