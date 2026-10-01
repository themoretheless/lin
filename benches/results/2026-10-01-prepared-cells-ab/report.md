# Rejected prepared cell-value cache

Three independent process pairs, alternating order; 12 fresh fixtures per case.

| Pair | Original full 10k | Cached cells | Original no embed | Cached no embed |
|---|---:|---:|---:|---:|
| 1 | 12.310 ms | 15.948 ms | 8.485 ms | 9.718 ms |
| 2 | 12.499 ms | 12.399 ms | 8.277 ms | 9.207 ms |
| 3 | 12.305 ms | 11.595 ms | 8.098 ms | 7.614 ms |

The candidate was removed: small wins in two full-insertion pairs did not
establish consistent improvement; one pair regressed substantially. Additional
retained memory is not justified. Background load is uncontrolled. Workspace
tests and synthetic time/duplicate-field checks passed. Source variants, hashes
and raw runs are saved. No peer win is claimed.

These runs predate the subsequent large-insert WAL/affected-count correctness
fix. Actual in-memory rows were inserted, but affected-count and large WAL
behavior were not validated by these diagnostic phase benchmarks.
