# Direct scalar-index iteration: retained

Store::index_insert_row now iterates BTreeMap values_mut directly instead of allocating a Vec<String>, cloning matching index labels, and looking up each label again. Filtering, label order, first-error handling and unique-index semantics remain unchanged. Public row and persistence formats are unchanged. This is a removal of temporary allocations, not a measured byte/RSS reduction.

Baseline and candidate came from one frozen snapshot, only src/store.rs differing. The complete default workspace tests passed on that snapshot, including uniqueness, rollback, delete/readback, cold rows, checkpoint and WAL. Production integration is a narrow method replacement preserving other concurrent work; final live test status is recorded separately in tests-live.log and validation.json.

Six alternating independent process pairs, 16 fresh fixtures per case and one timed operation each. Setup/drop and existing affected-row/readback validation are outside timing. Our builds and tests ended before timing. Unrelated host jobs remain uncontrolled and their inventory is retained. SQLite controls changed substantially in several pairs, so these measurements do not isolate a causal percentage or establish statistical significance.

## Paired outcomes

| Case | Candidate wins / 6 | Median paired time reduction |
|---|---:|---:|
| update_1row_1k/lin | 4 | 15.3% |
| delete_1row_1k/lin | 3 | 2.3% |
| update_1row_10k/lin | 4 | 3.1% |
| delete_1row_10k/lin | 3 | 1.2% |
| update_1row_100k/lin | 5 | 2.3% |
| delete_1row_100k/lin | 3 | 1.9% |

100k updates won 5/6 baseline comparisons; the reduction is modest (2.3% median paired). At 1k and 10k the update candidate won 4/6. Deletion won only 3/6 at every size, so a repeatable delete gain is not proved. This change is retained for simpler allocation-free index dispatch and the positive update observations, with no claim of stable deletion acceleration.

## All measured medians

| Pair | Case | Baseline µs | Direct iteration µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.667 | 4.041 | 13.4% |
| 1 | update_1row_1k/sqlite | 3.792 | 4.000 | -5.5% |
| 1 | delete_1row_1k/lin | 7.500 | 6.833 | 8.9% |
| 1 | delete_1row_1k/sqlite | 5.292 | 3.167 | 40.2% |
| 1 | update_1row_10k/lin | 9.250 | 8.583 | 7.2% |
| 1 | update_1row_10k/sqlite | 12.021 | 11.771 | 2.1% |
| 1 | delete_1row_10k/lin | 14.500 | 15.062 | -3.9% |
| 1 | delete_1row_10k/sqlite | 11.500 | 11.688 | -1.6% |
| 1 | update_1row_100k/lin | 14.521 | 12.666 | 12.8% |
| 1 | update_1row_100k/sqlite | 16.645 | 17.084 | -2.6% |
| 1 | delete_1row_100k/lin | 20.750 | 27.291 | -31.5% |
| 1 | delete_1row_100k/sqlite | 16.770 | 19.041 | -13.5% |
| 2 | update_1row_1k/lin | 6.167 | 4.354 | 29.4% |
| 2 | update_1row_1k/sqlite | 5.667 | 3.979 | 29.8% |
| 2 | delete_1row_1k/lin | 5.812 | 8.291 | -42.6% |
| 2 | delete_1row_1k/sqlite | 4.938 | 3.146 | 36.3% |
| 2 | update_1row_10k/lin | 8.479 | 8.291 | 2.2% |
| 2 | update_1row_10k/sqlite | 10.146 | 11.188 | -10.3% |
| 2 | delete_1row_10k/lin | 14.604 | 13.979 | 4.3% |
| 2 | delete_1row_10k/sqlite | 10.125 | 10.646 | -5.1% |
| 2 | update_1row_100k/lin | 13.396 | 11.479 | 14.3% |
| 2 | update_1row_100k/sqlite | 16.667 | 17.520 | -5.1% |
| 2 | delete_1row_100k/lin | 29.562 | 23.396 | 20.9% |
| 2 | delete_1row_100k/sqlite | 17.104 | 17.562 | -2.7% |
| 3 | update_1row_1k/lin | 5.729 | 5.958 | -4.0% |
| 3 | update_1row_1k/sqlite | 3.875 | 2.562 | 33.9% |
| 3 | delete_1row_1k/lin | 6.583 | 8.312 | -26.3% |
| 3 | delete_1row_1k/sqlite | 5.021 | 2.792 | 44.4% |
| 3 | update_1row_10k/lin | 8.396 | 9.271 | -10.4% |
| 3 | update_1row_10k/sqlite | 11.041 | 11.000 | 0.4% |
| 3 | delete_1row_10k/lin | 13.875 | 14.125 | -1.8% |
| 3 | delete_1row_10k/sqlite | 8.625 | 11.125 | -29.0% |
| 3 | update_1row_100k/lin | 13.459 | 12.958 | 3.7% |
| 3 | update_1row_100k/sqlite | 15.209 | 17.750 | -16.7% |
| 3 | delete_1row_100k/lin | 20.438 | 24.230 | -18.6% |
| 3 | delete_1row_100k/sqlite | 17.250 | 17.959 | -4.1% |
| 4 | update_1row_1k/lin | 4.625 | 3.833 | 17.1% |
| 4 | update_1row_1k/sqlite | 4.083 | 3.625 | 11.2% |
| 4 | delete_1row_1k/lin | 7.458 | 6.729 | 9.8% |
| 4 | delete_1row_1k/sqlite | 3.896 | 3.042 | 21.9% |
| 4 | update_1row_10k/lin | 7.792 | 7.896 | -1.3% |
| 4 | update_1row_10k/sqlite | 9.729 | 7.542 | 22.5% |
| 4 | delete_1row_10k/lin | 14.771 | 14.021 | 5.1% |
| 4 | delete_1row_10k/sqlite | 10.625 | 7.167 | 32.6% |
| 4 | update_1row_100k/lin | 13.041 | 14.584 | -11.8% |
| 4 | update_1row_100k/sqlite | 21.979 | 22.312 | -1.5% |
| 4 | delete_1row_100k/lin | 28.562 | 27.125 | 5.0% |
| 4 | delete_1row_100k/sqlite | 19.250 | 18.708 | 2.8% |
| 5 | update_1row_1k/lin | 5.833 | 3.458 | 40.7% |
| 5 | update_1row_1k/sqlite | 5.104 | 2.917 | 42.9% |
| 5 | delete_1row_1k/lin | 7.646 | 7.979 | -4.4% |
| 5 | delete_1row_1k/sqlite | 5.938 | 3.563 | 40.0% |
| 5 | update_1row_10k/lin | 8.875 | 7.854 | 11.5% |
| 5 | update_1row_10k/sqlite | 10.334 | 9.292 | 10.1% |
| 5 | delete_1row_10k/lin | 14.959 | 13.250 | 11.4% |
| 5 | delete_1row_10k/sqlite | 9.500 | 11.666 | -22.8% |
| 5 | update_1row_100k/lin | 13.979 | 13.979 | 0.0% |
| 5 | update_1row_100k/sqlite | 16.645 | 17.479 | -5.0% |
| 5 | delete_1row_100k/lin | 22.959 | 19.563 | 14.8% |
| 5 | delete_1row_100k/sqlite | 19.041 | 18.709 | 1.7% |
| 6 | update_1row_1k/lin | 4.167 | 6.021 | -44.5% |
| 6 | update_1row_1k/sqlite | 6.042 | 4.271 | 29.3% |
| 6 | delete_1row_1k/lin | 12.896 | 10.396 | 19.4% |
| 6 | delete_1row_1k/sqlite | 3.521 | 4.146 | -17.8% |
| 6 | update_1row_10k/lin | 8.729 | 8.375 | 4.1% |
| 6 | update_1row_10k/sqlite | 12.500 | 10.479 | 16.2% |
| 6 | delete_1row_10k/lin | 14.458 | 15.604 | -7.9% |
| 6 | delete_1row_10k/sqlite | 12.771 | 13.500 | -5.7% |
| 6 | update_1row_100k/lin | 16.000 | 15.854 | 0.9% |
| 6 | update_1row_100k/sqlite | 17.375 | 22.291 | -28.3% |
| 6 | delete_1row_100k/lin | 22.062 | 22.354 | -1.3% |
| 6 | delete_1row_100k/sqlite | 16.792 | 17.083 | -1.7% |

## Peer status

Candidate-process Lin versus SQLite wins out of six: {'compare/update_1row_1k/lin': 0, 'compare/delete_1row_1k/lin': 0, 'compare/update_1row_10k/lin': 5, 'compare/delete_1row_10k/lin': 0, 'compare/update_1row_100k/lin': 6, 'compare/delete_1row_100k/lin': 0}. Native deletion still loses SQLite; an all-case victory remains unproved. The broader DuckDB/PostgreSQL/MySQL/MSSQL/Mongo/Kusto/pandas objective remains active and incomplete. See peer-environment.md for verified remaining MSSQL/Kusto platform/configuration gaps.

Build-only Cargo calls intentionally selected no cases and returned exit 1 after successful compilation. All 12 measured processes completed successfully. Compared source variants, common/binary hashes, build/test logs and raw run.json files are preserved.
