# Posting-tail experiment and corrected timestamp fixtures

## Retained change: benchmark input parity

Warm Lin fixtures used `ago 1d`/`ago 30d` independently for each 500-row seed chunk. SQLite received the two absolute timestamps from Doc. Lin therefore could have many distinct [wing, ts] keys instead of the two used by SQLite; elapsed seeding time changed the index shape. Earlier warm-fixture peer timings using that loader do not establish exact timestamp/key-distribution parity. This does not identify a code regression or invalidate unrelated harnesses, but those earlier numbers cannot prove the current corrected native-write comparison.

The retained bench change anchors Doc timestamps once per benchmark process for all engines/fixtures. After Lin warm fixture loading, exact Cell::Time values are assigned from the same Doc records and scalar indexes are rebuilt outside timing. Every ID/timestamp pair and exactly two composite keys are checked before measurement. This only changes prepared read/write fixture setup; timed native ingestion still uses the public relative-time syntax and has not gained exact timestamp-value parity from this change. No optimization feature was disabled.

## Index candidates: not retained

The experiment checks the posting tail first, then uses the original linear search when necessary. The broad variant used this in both removal and key replacement. The narrow variant only changed remove_at, keeping insert_key unchanged. Unsorted unique posting lists have the same final vector/order as the original swap_remove algorithm; a new oracle test exercised empty/missing/tail/interior removal on shuffled lists through length 4096. The complete snapshot default workspace suite passed. Source variants are kept as experimental artifacts; production src/index.rs remains the original.

Fresh 100k deletion improved after input normalization, but smaller cases and steady-state writes did not give a consistent overall win. Narrow steady-state churn lost 2/3 pairs, so it was rejected. Broad normalized updates and small deletes were also mixed/negative. We do not claim these timing differences are isolated causal regressions: SQLite controls and shared host load varied too. No statistical significance is established.

## Corrected fixtures: broad candidate

Three independent prebuilt process pairs, alternating order; 24 fresh fixtures, one operation each. Setup/destruction and affected-row/readback checks are outside timing. Compiler/tests completed before measurements.

| Pair | Case | Baseline µs | Candidate µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 3.188 | 3.854 | -20.9% |
| 1 | update_1row_1k/sqlite | 4.146 | 6.333 | -52.8% |
| 1 | delete_1row_1k/lin | 8.916 | 10.083 | -13.1% |
| 1 | delete_1row_1k/sqlite | 4.688 | 4.562 | 2.7% |
| 1 | update_1row_10k/lin | 8.792 | 9.854 | -12.1% |
| 1 | update_1row_10k/sqlite | 12.812 | 12.396 | 3.3% |
| 1 | delete_1row_10k/lin | 16.229 | 16.854 | -3.9% |
| 1 | delete_1row_10k/sqlite | 10.146 | 13.937 | -37.4% |
| 1 | update_1row_100k/lin | 12.396 | 14.209 | -14.6% |
| 1 | update_1row_100k/sqlite | 16.916 | 19.104 | -12.9% |
| 1 | delete_1row_100k/lin | 36.605 | 21.375 | 41.6% |
| 1 | delete_1row_100k/sqlite | 19.999 | 18.584 | 7.1% |
| 2 | update_1row_1k/lin | 3.833 | 4.479 | -16.8% |
| 2 | update_1row_1k/sqlite | 3.521 | 3.916 | -11.2% |
| 2 | delete_1row_1k/lin | 4.875 | 5.833 | -19.7% |
| 2 | delete_1row_1k/sqlite | 3.417 | 3.750 | -9.7% |
| 2 | update_1row_10k/lin | 8.688 | 8.167 | 6.0% |
| 2 | update_1row_10k/sqlite | 9.666 | 11.917 | -23.3% |
| 2 | delete_1row_10k/lin | 14.812 | 14.334 | 3.2% |
| 2 | delete_1row_10k/sqlite | 9.959 | 12.062 | -21.1% |
| 2 | update_1row_100k/lin | 12.021 | 12.959 | -7.8% |
| 2 | update_1row_100k/sqlite | 17.146 | 18.188 | -6.1% |
| 2 | delete_1row_100k/lin | 34.896 | 22.709 | 34.9% |
| 2 | delete_1row_100k/sqlite | 17.729 | 16.646 | 6.1% |
| 3 | update_1row_1k/lin | 4.479 | 5.292 | -18.2% |
| 3 | update_1row_1k/sqlite | 5.062 | 5.688 | -12.4% |
| 3 | delete_1row_1k/lin | 7.125 | 6.396 | 10.2% |
| 3 | delete_1row_1k/sqlite | 4.521 | 5.250 | -16.1% |
| 3 | update_1row_10k/lin | 9.062 | 9.271 | -2.3% |
| 3 | update_1row_10k/sqlite | 10.458 | 13.229 | -26.5% |
| 3 | delete_1row_10k/lin | 15.146 | 16.458 | -8.7% |
| 3 | delete_1row_10k/sqlite | 10.875 | 12.666 | -16.5% |
| 3 | update_1row_100k/lin | 12.125 | 14.979 | -23.5% |
| 3 | update_1row_100k/sqlite | 16.333 | 19.938 | -22.1% |
| 3 | delete_1row_100k/lin | 34.062 | 24.750 | 27.3% |
| 3 | delete_1row_100k/sqlite | 18.730 | 19.750 | -5.4% |

## Corrected fixtures: removal-only candidate

Same fixture/timer contract; process order was reversed relative to the broad cohort.

| Pair | Case | Baseline µs | Candidate µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.354 | 6.229 | -43.1% |
| 1 | update_1row_1k/sqlite | 3.771 | 6.083 | -61.3% |
| 1 | delete_1row_1k/lin | 8.625 | 10.562 | -22.5% |
| 1 | delete_1row_1k/sqlite | 7.792 | 7.062 | 9.4% |
| 1 | update_1row_10k/lin | 8.688 | 11.041 | -27.1% |
| 1 | update_1row_10k/sqlite | 8.479 | 13.562 | -59.9% |
| 1 | delete_1row_10k/lin | 14.917 | 17.000 | -14.0% |
| 1 | delete_1row_10k/sqlite | 10.062 | 12.042 | -19.7% |
| 1 | update_1row_100k/lin | 15.167 | 18.521 | -22.1% |
| 1 | update_1row_100k/sqlite | 20.458 | 21.374 | -4.5% |
| 1 | delete_1row_100k/lin | 39.041 | 26.542 | 32.0% |
| 1 | delete_1row_100k/sqlite | 18.959 | 19.458 | -2.6% |
| 2 | update_1row_1k/lin | 6.417 | 5.604 | 12.7% |
| 2 | update_1row_1k/sqlite | 7.687 | 5.896 | 23.3% |
| 2 | delete_1row_1k/lin | 7.417 | 7.250 | 2.2% |
| 2 | delete_1row_1k/sqlite | 5.042 | 4.771 | 5.4% |
| 2 | update_1row_10k/lin | 9.562 | 9.500 | 0.6% |
| 2 | update_1row_10k/sqlite | 12.500 | 12.521 | -0.2% |
| 2 | delete_1row_10k/lin | 17.792 | 15.146 | 14.9% |
| 2 | delete_1row_10k/sqlite | 13.771 | 11.291 | 18.0% |
| 2 | update_1row_100k/lin | 14.479 | 13.167 | 9.1% |
| 2 | update_1row_100k/sqlite | 20.688 | 16.688 | 19.3% |
| 2 | delete_1row_100k/lin | 41.478 | 24.791 | 40.2% |
| 2 | delete_1row_100k/sqlite | 19.666 | 14.541 | 26.1% |
| 3 | update_1row_1k/lin | 5.812 | 4.146 | 28.7% |
| 3 | update_1row_1k/sqlite | 5.896 | 3.583 | 39.2% |
| 3 | delete_1row_1k/lin | 8.041 | 9.292 | -15.6% |
| 3 | delete_1row_1k/sqlite | 3.896 | 3.646 | 6.4% |
| 3 | update_1row_10k/lin | 10.521 | 9.834 | 6.5% |
| 3 | update_1row_10k/sqlite | 12.875 | 11.708 | 9.1% |
| 3 | delete_1row_10k/lin | 27.062 | 13.791 | 49.0% |
| 3 | delete_1row_10k/sqlite | 17.291 | 9.584 | 44.6% |
| 3 | update_1row_100k/lin | 16.166 | 12.916 | 20.1% |
| 3 | update_1row_100k/sqlite | 19.875 | 14.479 | 27.1% |
| 3 | delete_1row_100k/lin | 40.729 | 21.541 | 47.1% |
| 3 | delete_1row_100k/sqlite | 18.188 | 15.521 | 14.7% |

## Steady-state diagnostics

10k rows, ten-second intervals per prebuilt process; prepared delete/reinsert pairs rotate IDs. Count, exact ID/URI readback, shared-token exact IDs/multiplicity are validated after the timer. This is not a peer benchmark.

Broad variant:

| Pair | Baseline pairs/s | Candidate pairs/s | Gain |
|---|---:|---:|---:|
| 1 | 108947.9 | 141506.8 | 29.88% |
| 2 | 152279.5 | 169275.9 | 11.16% |
| 3 | 118795.3 | 155537.7 | 30.93% |

Removal-only variant:

| Pair | Baseline pairs/s | Candidate pairs/s | Gain |
|---|---:|---:|---:|
| 1 | 171165.2 | 171633.7 | 0.27% |
| 2 | 171942.7 | 161226.3 | -6.23% |
| 3 | 166580.5 | 123027.5 | -26.15% |

## Evidence and remaining work

- fixed-* is the normalized broad cohort; only-* is the normalized removal-only cohort.
- pair-* predates timestamp normalization and is exploratory, not a corrected peer proof.
- fixed-read confirms a normalized scalar read fixture passed its assertions.
- live-fixed-check verifies retained benchmark setup against current production code.
- All measured snapshot processes completed successfully, as did churn readback validators. Build-only Cargo filters intentionally selected no cases and returned 1 after compilation.
- Shared source contexts, variants, source archives, binaries/hashes, raw observations and background process inventory are retained.

The retained production change is benchmark correctness, not a library speedup. No eight-peer victory is claimed. Native deletion still requires improvement; exact-time native ingestion plus MSSQL/Kusto environments remain outstanding.

Live production verification: compare/delete_1row_100k/lin: 24.375 µs, compare/delete_1row_100k/sqlite: 19.249 µs (one process, 12 fresh fixtures; not an independent paired optimization proof). Fixture assertions and readback passed; scoped rustfmt/diff checks passed.
