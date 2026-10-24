# Prepared filtered COUNT uses existing fast paths

Baseline source `28af2dc0107e82cecfc49fd7ee3ed6a27d988b2c`; candidate based on evidence-only `6fe245f` with the same production source. Reused baseline binary verified against prior current-read-refresh SHA256; exact before/candidate src/exec.rs and binary/source hashes retained. Candidate source hashes verified unchanged after all runs.

## Change

Prepared aggregate dispatch previously reached execute_agg_plan's generic row-map COUNT scan before exec_query_lim could use try_filter_count. Extract the existing filter-count implementation into one shared helper and invoke it for prepared filtered ungrouped COUNT. Index-covered predicates now count posting lists; title substring uses the existing packed physical title column. Residual predicates retain row evaluation. No result cache, added indexes, schema, embedding/FTS, grouping, durability or wire format changes. Existing physical title packing invalidation is exercised after updates/deletes/inserts.

## Validation

`cargo test --workspace` and release peer_bench build passed (checks.json). New integration coverage checks mutable and readonly prepared results against explicit expected counts; duplicate OR branches, indexed residual predicates, repeated matches per row, cross-row false matches, Unicode-bearing rows, empty substring, update/delete/insert invalidation, catalog visibility and explicit `all`. Existing workspace tests cover replay/rollback and other aggregate behavior. Benchmark fixtures use independent exact value/multiplicity validation outside timing.

## Measurement

Two independent series, each six alternating baseline/candidate process pairs at each of 1k/10k/100k. Each process compares Lin, SQLite, DuckDB and pandas on filter_eq and text_substr, 12 samples, target sample 5ms, one process per run. Fresh fixtures/setup/validation/JSON IPC excluded; engine order fixed within each process and baseline/candidate order alternated. Controls run in both variants. No builds/tests initiated by this runner overlap measurement; unrelated host activity was not controlled. Warm API measurements, not intrinsic CPU or operation-level p99. Lin prepared Rust API versus Python driver/DataFrame interfaces; see docs/benchmark-contract.md. Runtime versions and raw samples retained in all run.json files.

Every run completed with no validation errors. Baseline exit 3 from --require-wins is an expected measured loss, not a crash. All 36 candidate processes exit 0: **216/216 process-level peer comparisons won**, representing 18 distinct size/case/peer comparisons repeated 12 times. These are two count cases only; they do not prove the full read matrix or all named peers.

| Series | Rows | Case | Baseline Lin µs | Candidate Lin µs | Paired gain | Candidate vs baseline | Candidate vs peers |
|---|---:|---|---:|---:|---:|---:|---:|
| 1 | 1000 | filter_eq | 26.280 | 0.656 | 97.533% | 6/6 | 18/18 |
| 1 | 1000 | text_substr | 27.173 | 1.858 | 93.314% | 6/6 | 18/18 |
| 1 | 10000 | filter_eq | 275.562 | 0.678 | 99.754% | 6/6 | 18/18 |
| 1 | 10000 | text_substr | 299.182 | 16.669 | 94.240% | 6/6 | 18/18 |
| 1 | 100000 | filter_eq | 6169.448 | 0.841 | 99.986% | 6/6 | 18/18 |
| 1 | 100000 | text_substr | 6300.875 | 207.317 | 96.705% | 6/6 | 18/18 |
| 2 | 1000 | filter_eq | 25.861 | 0.659 | 97.429% | 6/6 | 18/18 |
| 2 | 1000 | text_substr | 26.937 | 1.889 | 93.119% | 6/6 | 18/18 |
| 2 | 10000 | filter_eq | 264.193 | 0.678 | 99.742% | 6/6 | 18/18 |
| 2 | 10000 | text_substr | 290.176 | 16.705 | 94.196% | 6/6 | 18/18 |
| 2 | 100000 | filter_eq | 5738.562 | 0.857 | 99.985% | 6/6 | 18/18 |
| 2 | 100000 | text_substr | 6133.938 | 209.880 | 96.532% | 6/6 | 18/18 |

SQLite/DuckDB/pandas control paired gains (positive=faster in candidate process):

| Series | Rows | Case | SQLite | DuckDB | pandas |
|---|---:|---|---:|---:|---:|
| 1 | 1000 | filter_eq | 0.689% | -1.516% | 2.193% |
| 1 | 1000 | text_substr | 3.130% | 3.068% | 1.080% |
| 1 | 10000 | filter_eq | 0.831% | 1.729% | 0.846% |
| 1 | 10000 | text_substr | 2.600% | 1.608% | 1.962% |
| 1 | 100000 | filter_eq | -0.169% | 5.696% | -0.806% |
| 1 | 100000 | text_substr | 3.892% | 3.092% | 1.087% |
| 2 | 1000 | filter_eq | 2.226% | 0.603% | 0.703% |
| 2 | 1000 | text_substr | -0.493% | -4.904% | 1.723% |
| 2 | 10000 | filter_eq | -0.122% | -1.366% | 1.639% |
| 2 | 10000 | text_substr | 0.571% | -0.971% | -0.476% |
| 2 | 100000 | filter_eq | 2.330% | -1.276% | 2.561% |
| 2 | 100000 | text_substr | 1.725% | 0.129% | 0.108% |

## Decision and remaining scope

Retain: large repeated gains in all 12 pairs per case/size, independent exact validation, complete workspace test. No peak RSS was collected, and memory performance is unqualified. Next rerun all six read cases on the candidate and re-establish dedicated PostgreSQL/MySQL/MongoDB measurements. MSSQL/Kusto still require configured dedicated endpoints; missing results are not wins. Durable insertion SQLite gaps remain separate and unresolved.
