# Prepare-time sorted insertion records — rejected

Candidate stably sorts insert fields and collapses duplicates once after typecheck/plan during prepare (last value retained). Bulk execution collects the already sorted fields into BTreeMap. Relative timestamps still evaluated at execution; public source and plan retained from original program. Preparation work lies outside native insert timers under the existing prepared-query contract; this is a real compilation change, not disabled work.

Full offline workspace tests pass. Additional prepared insert test covers three duplicate title fields, deterministic evaluation at two supplied times, actual execution-time now and exact stored value. Test retained after reverting optimization. Current native profile in ../2026-10-01-native-profile-current/ motivated construction-path investigation; diagnostic is not peer timing evidence.

Six alternating process pairs, 24 fresh fixtures/case, one operation. Full native Lin, SQLite, DuckDB Appender; exact fields checked outside timer. Pinned optimized binaries; tests/builds did not overlap measurements. Host load uncontrolled, fixed case order. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 0.962812 | 0.977792 | 10.428895 | 10.344687 |
| 2 | 0.902437 | 0.929437 | 9.794854 | 10.362646 |
| 3 | 0.948896 | 0.945750 | 10.309126 | 9.994687 |
| 4 | 0.926729 | 0.918583 | 9.719188 | 9.942291 |
| 5 | 0.926000 | 0.895020 | 9.826291 | 9.796229 |
| 6 | 0.919312 | 0.900896 | 10.111688 | 9.852208 |

Both sizes win 4/6 pairs; median paired decreases 0.61% and 0.56%. SQLite controls median -0.91% and +0.97%, DuckDB +0.93% and +2.47%; no stable improvement beyond uncontrolled variation established. Candidate rejected; original prepare and row construction restored. Prior WAL/doc slab changes retained. All-eight-peer objective unproven. Raw observations and hashes retained.
