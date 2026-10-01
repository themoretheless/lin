# Shared identity keys: retained for ingestion, mutation performance unresolved

Private by-ID and document-URI maps now hold Arc<str> keys sharing immutable cell text instead of allocating String copies. Public Row and persisted row/column formats are unchanged. Rebuild, single registration and slab registration use shared keys.

Initial live-workspace builds were rejected because src/exec.rs changed concurrently. Actual compared binaries were built from a frozen source snapshot with only src/store.rs differing. Snapshot build logs, common hashes and source-snapshot.tar.gz preserve this context. Default features only; no GPU benchmark claim is made.

## Six paired bulk and point-read processes

16 fresh fixtures per insertion phase; 24 calibrated warm point-read observations per process. Builds completed before measurement. Alternating process order. All phases still register identity maps; there is no unchanged identity-map control.

| Pair | Baseline full ms | Shared full ms | Paired reduction | Baseline point ns | Shared point ns |
|---|---:|---:|---:|---:|---:|
| 1 | 11.027 | 11.275 | -2.3% | 253.57 | 242.83 |
| 2 | 12.155 | 11.312 | 6.9% | 234.66 | 244.50 |
| 3 | 12.242 | 11.139 | 9.0% | 232.18 | 240.76 |
| 4 | 13.483 | 16.177 | -20.0% | 285.41 | 297.17 |
| 5 | 13.845 | 11.975 | 13.5% | 263.77 | 253.35 |
| 6 | 14.163 | 12.307 | 13.1% | 303.96 | 289.84 |

Median paired full-insert reduction 8.0%, 4/6 wins. Point-read median paired change 0.13%, effectively tied with 3/6 wins. Load is uncontrolled and timings fluctuate. No statistical significance or universal causal percentage is claimed.

## Matched single-row writes

Three independent process pairs; 12 fresh fixtures per case. Validators outside timing. These expose an unresolved tradeoff rather than a confirmed single-row improvement.

| Pair | Case | Baseline Lin µs | Shared Lin µs | Baseline SQLite µs | Shared-process SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | update 1k | 3.999 | 5.625 | 4.667 | 4.646 |
| 1 | update 10k | 8.688 | 8.667 | 10.979 | 8.416 |
| 1 | update 100k | 14.501 | 13.438 | 15.958 | 15.562 |
| 1 | delete 1k | 6.333 | 9.521 | 3.062 | 4.021 |
| 1 | delete 10k | 13.500 | 13.521 | 9.834 | 7.458 |
| 1 | delete 100k | 21.541 | 23.062 | 13.062 | 18.666 |
| 2 | update 1k | 8.312 | 6.104 | 4.312 | 5.354 |
| 2 | update 10k | 9.354 | 8.334 | 14.000 | 11.625 |
| 2 | update 100k | 13.833 | 15.771 | 19.854 | 14.896 |
| 2 | delete 1k | 11.583 | 9.834 | 6.333 | 4.167 |
| 2 | delete 10k | 17.375 | 14.666 | 15.583 | 9.979 |
| 2 | delete 100k | 21.895 | 22.812 | 14.166 | 19.084 |
| 3 | update 1k | 5.062 | 6.041 | 3.146 | 4.854 |
| 3 | update 10k | 8.209 | 8.896 | 9.166 | 9.374 |
| 3 | update 100k | 13.688 | 15.937 | 18.729 | 19.500 |
| 3 | delete 1k | 6.021 | 5.146 | 3.979 | 4.750 |
| 3 | delete 10k | 14.479 | 14.250 | 7.938 | 8.999 |
| 3 | delete 100k | 23.750 | 26.813 | 17.854 | 19.208 |

100k update lost two of three baseline comparisons. Shared-process Lin beat SQLite in two of three 100k update runs, but a stable peer win is not proved. Single-row performance requires further work; it is not being marked complete.

## URI-only alternative

Keeping String IDs while sharing only URI keys won only 1/3 full insertion pairs; it was not selected. Corresponding write timings are also included in the uri-write raw runs.

| Pair | Baseline full ms | URI-only full ms | Reduction |
|---|---:|---:|---:|
| 1 | 12.222 | 12.433 | -1.7% |
| 2 | 18.713 | 13.818 | 26.2% |
| 3 | 12.031 | 12.622 | -4.9% |

## Native bulk snapshot verification

One process, 12 fresh fixtures, count/row readback outside timing; Appender flush inside timing. Setup/schema differences are documented in the contract. These timings prove no complete peer victory.

| Rows | Shared Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.039 | 0.810 | 1.179 |
| 10k | 11.002 | 10.286 | 10.219 |

Lin still loses 10k ingestion; native deletion also remains slower than SQLite. The first native-delete run overlapped workspace compilation/tests and is explicitly excluded. native-delete-clean ran after those checks ended, with passing validators; its raw timings remain separate from matched write comparisons.

## Validation and decision

Complete default workspace tests passed on the current working tree after the shared-key change, including rollback, cold readback, checkpoint/reopen, FTS and WAL. The shared-key implementation is retained as a partial ingestion/memory improvement while the single-row tradeoff remains unresolved. This does not close the objective or justify dropping update/delete requirements. Existing confirmed FTS/ASCII/normalization changes remain. Scoped rustfmt and git diff --check pass.
