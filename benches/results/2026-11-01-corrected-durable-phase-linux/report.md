# Current COW durable caller-thread phase diagnostic

All 240 fresh processes /1920 measured wall observations retained. Four roles: original/control share one binary; disabled/traced share the instrumented binary. Phase fractions are same-invocation ratios, aggregated within then across processes. Trace records include validation/calibration; no per-observation alignment is claimed. RSS includes fixture, oracle, reopen and tracing. Checkpoint is an inclusive parent; no nested totals are summed. No source optimization or peer superiority is qualified.

|Case|A/A %|Disabled/original|Traced/disabled|Trace records|Payload bytes|Exec %|Encode %|Write %|Sync %|Rollback %|
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|compare/durable_update_1row_1k/lin|6.497|1.003|0.967|66|4112.0|5.677|1.826|6.719|83.578|0.000|
|compare/durable_delete_1row_1k/lin|7.069|1.008|0.983|66|4112.0|8.242|1.795|7.359|79.903|0.000|
|compare/durable_commit_1row_1k/lin|4.427|0.998|0.946|66|4112.0|5.604|1.798|6.776|83.207|0.000|
|compare/durable_rollback_1row_1k/lin|4.515|1.033|1.022|66|0.0|52.721|0.000|0.000|0.000|33.532|
|compare/durable_update_1row_10k/lin|3.964|1.022|0.988|66|4112.0|5.121|1.338|9.011|82.437|0.000|
|compare/durable_delete_1row_10k/lin|3.741|1.015|0.999|66|4112.0|7.245|1.334|8.684|80.685|0.000|
|compare/durable_commit_1row_10k/lin|4.699|1.000|0.991|66|4112.0|4.798|1.353|8.915|83.250|0.000|
|compare/durable_rollback_1row_10k/lin|1.197|1.035|1.011|66|0.0|55.562|0.000|0.000|0.000|31.707|
|compare/wal_scalar_full_update/lin|7.564|1.014|0.968|66|104.0|2.417|0.561|2.152|93.955|0.000|
|compare/wal_scalar_normal_update/lin|7.514|1.234|0.947|66|104.0|53.939|13.874|16.618|0.000|0.000|
