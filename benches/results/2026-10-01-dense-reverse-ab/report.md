# Dense reverse scalar-index keys: rejected

An isolated candidate replaced the row-position FxHashMap<usize, IndexKey> with Vec<Option<IndexKey>>. Forward BTree postings, query bounds, numeric ordering, uniqueness checks, and remove/reinsert posting order were preserved. Holes supported updates, swap-removal and rollback. Production sources were not edited.

The complete default workspace suite passed. An additional 3,000-edit oracle covered insertion, replacement, deletion, sparse positions through 1024, clone independence and out-of-range removal. Pure dense insertion of extremely sparse arbitrary positions could allocate excessively, another issue that would need a bounded/sparse fallback before any production adoption.

Baseline and candidate were compiled from a frozen snapshot; only src/index.rs differed, including the extra test. Six independent prebuilt processes ran in three alternating pairs, with writes (12 fresh samples), insert phases (16 fresh samples), and warm scalar reads (24 calibrated samples). Mutation/insert operations were capped at one per fresh fixture. Existing count/readback validators ran outside timing. Builds and our tests ended before timing. Unrelated cargo/rustc processes were running on this shared host; the process inventory is retained. The timing variation does not establish statistical significance or an isolated causal percentage.

## Single-row writes

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.812 | 3.292 | 31.6% |
| 1 | update_1row_1k/sqlite | 5.125 | 3.354 | 34.5% |
| 1 | delete_1row_1k/lin | 10.959 | 8.709 | 20.5% |
| 1 | delete_1row_1k/sqlite | 8.791 | 4.542 | 48.3% |
| 1 | update_1row_10k/lin | 9.625 | 8.875 | 7.8% |
| 1 | update_1row_10k/sqlite | 14.563 | 9.875 | 32.2% |
| 1 | delete_1row_10k/lin | 16.709 | 13.062 | 21.8% |
| 1 | delete_1row_10k/sqlite | 12.125 | 11.562 | 4.6% |
| 1 | update_1row_100k/lin | 13.291 | 24.500 | -84.3% |
| 1 | update_1row_100k/sqlite | 23.499 | 18.250 | 22.3% |
| 1 | delete_1row_100k/lin | 28.041 | 33.167 | -18.3% |
| 1 | delete_1row_100k/sqlite | 19.062 | 18.375 | 3.6% |
| 2 | update_1row_1k/lin | 6.250 | 8.146 | -30.3% |
| 2 | update_1row_1k/sqlite | 5.188 | 7.458 | -43.8% |
| 2 | delete_1row_1k/lin | 7.938 | 13.438 | -69.3% |
| 2 | delete_1row_1k/sqlite | 5.854 | 7.167 | -22.4% |
| 2 | update_1row_10k/lin | 9.521 | 10.146 | -6.6% |
| 2 | update_1row_10k/sqlite | 13.354 | 11.313 | 15.3% |
| 2 | delete_1row_10k/lin | 18.646 | 16.291 | 12.6% |
| 2 | delete_1row_10k/sqlite | 12.188 | 16.959 | -39.1% |
| 2 | update_1row_100k/lin | 16.896 | 33.270 | -96.9% |
| 2 | update_1row_100k/sqlite | 18.084 | 23.646 | -30.8% |
| 2 | delete_1row_100k/lin | 23.834 | 41.874 | -75.7% |
| 2 | delete_1row_100k/sqlite | 18.354 | 22.584 | -23.0% |
| 3 | update_1row_1k/lin | 4.854 | 6.812 | -40.3% |
| 3 | update_1row_1k/sqlite | 4.312 | 3.958 | 8.2% |
| 3 | delete_1row_1k/lin | 8.812 | 5.688 | 35.5% |
| 3 | delete_1row_1k/sqlite | 4.667 | 4.229 | 9.4% |
| 3 | update_1row_10k/lin | 8.917 | 9.166 | -2.8% |
| 3 | update_1row_10k/sqlite | 10.667 | 11.271 | -5.7% |
| 3 | delete_1row_10k/lin | 14.312 | 14.437 | -0.9% |
| 3 | delete_1row_10k/sqlite | 13.021 | 9.500 | 27.0% |
| 3 | update_1row_100k/lin | 15.479 | 32.958 | -112.9% |
| 3 | update_1row_100k/sqlite | 19.396 | 18.333 | 5.5% |
| 3 | delete_1row_100k/lin | 23.980 | 36.416 | -51.9% |
| 3 | delete_1row_100k/sqlite | 19.104 | 16.729 | 12.4% |

## Insertion phases

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | insert_phase_10k/lin_full | 11777.208 | 15252.000 | -29.5% |
| 1 | insert_phase_10k/lin_no_embed | 8226.646 | 8850.833 | -7.6% |
| 1 | insert_phase_10k/lin_no_embed_no_scalar_index | 7093.395 | 8163.188 | -15.1% |
| 1 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 6147.374 | 6235.688 | -1.4% |
| 2 | insert_phase_10k/lin_full | 11338.729 | 18357.875 | -61.9% |
| 2 | insert_phase_10k/lin_no_embed | 7697.812 | 9989.833 | -29.8% |
| 2 | insert_phase_10k/lin_no_embed_no_scalar_index | 6773.938 | 8118.604 | -19.9% |
| 2 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5538.271 | 6148.000 | -11.0% |
| 3 | insert_phase_10k/lin_full | 12260.604 | 11849.146 | 3.4% |
| 3 | insert_phase_10k/lin_no_embed | 8133.730 | 7807.792 | 4.0% |
| 3 | insert_phase_10k/lin_no_embed_no_scalar_index | 7087.771 | 7130.999 | -0.6% |
| 3 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5625.958 | 6226.812 | -10.7% |

## Warm scalar equality read

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | filter_eq/lin | 1.039 | 1.150 | -10.7% |
| 2 | filter_eq/lin | 1.063 | 1.101 | -3.6% |
| 3 | filter_eq/lin | 1.063 | 1.137 | -6.9% |

## Decision

Rejected: no consistent improvement across required writes, insertion and reads, and no broad SQLite victory. The existing hash reverse map remains in the current workspace. Native write semantics, including undo/CAS/FTS, remain included; no feature was disabled to obtain timings. MSSQL/Kusto and the overall eight-peer goal remain unverified/incomplete. Build-only filtered Cargo calls exited 1 because they intentionally selected no cases after successful compilation; all 18 actual measurement runs completed with exit 0.
