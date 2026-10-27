# Decision: expanded COW source gate failed

Run40502052753 on frozen sourcefdc4a0ee completed all1332 processes/10656 measured observations. Independent raw audit passes; unchanged source acceptance gate fails. No production adoption, API migration or rerun of this unchanged failed candidate is justified. Original narrow720-process qualification remains historical evidence in its original scope.

## UPDATE targets

| Case | Series | Gain vs main % | Absolute A/A % | Positive/6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | 60.610 | 2.281 | 6 | True |
| compare/update_1row_1k/lin | series2 | 60.377 | 0.899 | 6 | True |
| compare/update_1row_10k/lin | series1 | 59.560 | 1.849 | 6 | True |
| compare/update_1row_10k/lin | series2 | 60.056 | 1.549 | 6 | True |
| compare/update_1row_100k/lin | series1 | 63.227 | 2.543 | 6 | True |
| compare/update_1row_100k/lin | series2 | 63.097 | 5.732 | 6 | True |

All six UPDATE targets pass, but target gains alone do not accept this source.

## Every failed control

| Case | Series | Gain % | Absolute A/A % | Positive/6 |
|---|---|---:|---:|---:|
| compare/tx_rollback_1row_1k/sqlite | series1 | -2.052 | 1.587 | 1 |
| compare/tx_rollback_1row_10k/sqlite | series2 | -2.005 | 0.921 | 1 |
| compare/durable_insert_1k/lin | series2 | -5.093 | 1.605 | 0 |
| compare/filter_eq/lin | series2 | -4.675 | 2.282 | 2 |
| compare/filter_eq/duckdb | series1 | -9.700 | 4.377 | 1 |
| compare/filter_eq/duckdb | series2 | -6.348 | 4.173 | 1 |

Two Lin controls fail: Full-durable insert1k series2 (-5.093%,0/6 positive,1.605% A/A) and equality COUNT series2 (-4.675%,2/6 positive,2.282% A/A). Four unchanged-peer controls also fail. No noisy peer control is dropped or treated as proof that the Lin regressions are harmless.

## Same-case numeric peer wins

Lin wins122/204 SQLite comparisons:62/120 native mutation/transaction,24/48 native/Full-durable insert,36/36 COUNT reads. Read DuckDB36/36. DELETE remains0/36 SQLite; native bulk insert0/24;10k rollback0/12. UPDATE wins1k12/12,10k8/12,100k11/12. These counts are source/host/workload-specific and do not qualify Full-durable mutations, arbitrary reads or the complete eight-peer goal. The newly measured durable mutation matrix is a separate production-source baseline; its timings must not be attributed to COW.

Potential next COW hypothesis: delay Arc-container allocation for newly constructed small/output/bulk rows, promote only a mutable row that must be shared. This is unimplemented and unmeasured, may change Row size/cache behavior or read-copy costs, and must preserve held-result isolation and the full output/undo/persistence contract. Do not claim Arc allocation caused these regressions without controlled evidence. New representation would require full qualification of its own source and final binary.

The owning adapters archive229a145e addresses explicit consuming APIs but changes source; this run does not qualify that later variant. Exact BTreeMap assignment is still incompatible. The human API decision remains pending and production Row remains unchanged.

Next main-path work remains the verified SQLite FULL WAL durable gap. Instrument actual caller-thread serialization/write/sync paths with original and instrumentation-disabled controls. Preserve every failed/negative result and all sync/index/FTS/embedding work. Full objective remains active and incomplete.
