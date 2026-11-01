# Controlled seed transaction history — frozen diagnostic protocol

Purpose: investigate whether the transaction history used to populate a durable fixture contributes to the remaining Lin / SQLite WAL gap. This is a counterfactual diagnostic, not adoption of a faster-looking fixture and not reinterpretation of original frozen results.

Use the exact columnar Batch prototype runtime, retaining fdatasync, with one executable for every mode. Selected case callbacks use the same run_durable_lin_write/run_durable_sqlite_write bodies and complete close/reopen oracles. Only setup selects separate 500-row Lin commits versus all identical insert programs in one transaction. SQLite retains its original single seed transaction under both mode labels as an A/A control. A shared executable removes separate-build differences between modes; it does not isolate all possible setup effects such as generation, allocation layout, metadata or cache state.

The initial draft build used original per-process FIXTURE_NOW; its source, build log and provenance are retained separately and do not qualify any timing. The revised diagnostic fixes the anchor at 1790000000000 milliseconds. Before timing verify actual full seed values agree between modes. Existing component benchmarks keep their original timestamp behavior and frozen source.

Coverage: 1k/10k, UPDATE/DELETE/COMMIT/ROLLBACK. Before any timing require all 32 selected benchmark smokes and eight Linux setup traces (two sizes, two modes, two engines for UPDATE), verifying that Lin seed WAL history actually differs while SQLite history stays unchanged. Mutation-only traces and all values remain retained. Do not claim that transaction grouping isolates a single physical sync cost.

Measurement: 12 rounds, rotated eight workloads and six roles per workload: Lin chunked baseline, identical Lin chunked A/A, an additional independent identical Lin null run, Lin grouped, SQLite chunked-label baseline, SQLite grouped-label control. Eight ordinary wall observations per fresh process, max iterations one, no warmup. 576 fresh processes / 4608 observations. Descriptive paired gains, absolute A/A and full raw values; no production acceptance gate or unchanged retry. Inputs, order and analyzers are frozen in frozen.json before Linux timing. Linux timing and Kusto/eight-peer completion remain unproven.


## Counterbalance correction

Run 41349896598 at 33c75b76675141f2d12a86c808ade7a75e3557d0 was cancelled during compilation, before its measurement step, after the case-rotation/role-rotation cancellation was confirmed in the original columnar Batch run. Its cancelled artifact and zero Linux measurement count are retained separately. No old measured outcome is converted into a pass.

The corrected scheduler uses each case's original frozen index, never its rotated processing position. Six roles use the six-row Williams design based on [0,1,5,2,4,3], with cyclic treatment offsets. In each six-round series every role occupies every position once; each pair has both precedence directions three times; each directed adjacency appears once. Independent property checks validate these counts before the exact returned schedule is executed, and repeat on raw process records afterwards. The separate three-role source benchmark planner uses all six permutations.

The extra Lin null role executes the exact same binary, filter and seed mode as baseline and A/A. Its median gain is evaluated against the earlier control tolerance -max(2 percent, absolute A/A) as a diagnostic of measurement/design variation, never as a source-code regression or an adoption gate. Retain failed null bands and all grouped-history results.
