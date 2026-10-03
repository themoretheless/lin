# Parallel bulk row construction: retained

Baseline: ff5fc07. Candidate builds input Rows before validation, using caller plus one scoped worker at batch sizes >=4096. Smaller batches build rows sequentially. ID allocation, content hash filling, duplicate ID/URI checks, foreign-key checks, embedding and index updates remain serial in original record order. One execution timestamp is shared across both halves; duplicate fields retain last-wins behavior. Public Row representation and wire/durability formats are unchanged.

Six independent alternating baseline/candidate pairs per workload, 24 fresh fixtures per process, one operation/sample and no warmup. Build/tests did not overlap measurement. Benchmark exact affected-count/value/readback checks passed for both batch sizes. Source and binary hashes and all raw run.json samples are saved here; final-exec.rs adds regression tests only after candidate measurement.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | -1.72% | 1/6 | 0.915177 | 0.928885 |
| compare/insert_bulk_1k/sqlite | -1.72% | 2/6 | 0.799615 | 0.813344 |
| compare/insert_bulk_10k/lin | 2.82% | 6/6 | 10.087312 | 9.789990 |
| compare/insert_bulk_10k/sqlite | -0.80% | 1/6 | 10.555396 | 10.583020 |
| compare/durable_insert_1k/lin | 1.21% | 3/6 | 2.502729 | 2.471146 |
| compare/durable_insert_1k/sqlite | 0.67% | 4/6 | 1.429167 | 1.425198 |
| compare/durable_insert_10k/lin | 5.37% | 6/6 | 19.880969 | 18.844136 |
| compare/durable_insert_10k/sqlite | -0.07% | 2/6 | 13.668125 | 13.679312 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Large native and durable workloads improved in all six pairs, while SQLite controls were flat/slower. The small native slowdown closely tracks its SQLite control; small durable results are mixed. These process comparisons do not prove performance on other hardware or document shapes.

Resource tradeoff: the large batch uses two CPU execution contexts rather than one during pure Row construction, then returns to serial execution. Thread creation and join happen per operation and are INCLUDED in insert timing. A temporary vector holds right-half row headers until merge, plus normal OS thread resources. Peak memory/CPU energy were not measured. This is a latency comparison with additional CPU parallelism, not a per-core throughput improvement. The 4096 threshold is a guard against small-batch thread overhead, not a demonstrated universally optimal crossover. All input rows are now constructed before key/FK validation, so invalid input may do extra allocation work before returning its unchanged validation error.

Validation: four existing focused record tests passed. New tests cover 4095/4096/4101 row boundaries, original row order, generated ID sequence, last duplicate fields, Unicode text, generated hashes, URI lookup and one shared execution-time value. Another test checks duplicate ID and URI errors crossing the worker boundary with no stored partial batch. Both new tests passed. Full `cargo test --offline --workspace` passed, including existing durability/reopen/rollback checks. `git diff --check` passed.

Decision: retain the measured large-batch latency improvement. Lin still loses to SQLite on native 1k and durable 1k/10k in these aggregate timings; native 10k favors Lin. MSSQL/Kusto endpoints remain absent. The full nine-engine objective remains incomplete.
