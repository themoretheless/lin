# Current COW durable caller-thread phase diagnostic

All 240 fresh processes /1920 measured wall observations retained. Four roles: original/control share one binary; disabled/traced share the instrumented binary. Phase fractions are same-invocation ratios, aggregated within then across processes. Trace records include validation/calibration; no per-observation alignment is claimed. RSS includes fixture, oracle, reopen and tracing. Checkpoint is an inclusive parent; no nested totals are summed. No source optimization or peer superiority is qualified.

|Case|A/A %|Disabled/original|Traced/disabled|Trace records|Payload bytes|Exec %|Encode %|Write %|Sync %|Rollback %|
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|compare/durable_update_1row_1k/lin|4.929|0.947|1.063|66|4112.0|3.866|1.570|6.359|87.031|0.000|
|compare/durable_delete_1row_1k/lin|2.040|1.068|0.965|66|4112.0|6.342|1.576|6.539|83.366|0.000|
|compare/durable_commit_1row_1k/lin|6.868|1.022|0.995|66|4112.0|4.186|1.664|6.745|85.634|0.000|
|compare/durable_rollback_1row_1k/lin|15.643|0.937|1.147|66|0.0|48.921|0.000|0.000|0.000|38.817|
|compare/durable_update_1row_10k/lin|5.400|1.025|1.001|66|4112.0|4.522|1.301|8.410|84.058|0.000|
|compare/durable_delete_1row_10k/lin|3.693|1.024|0.974|66|4112.0|7.153|1.303|8.530|80.986|0.000|
|compare/durable_commit_1row_10k/lin|7.874|1.027|0.984|66|4112.0|4.826|1.311|9.002|83.051|0.000|
|compare/durable_rollback_1row_10k/lin|2.130|1.017|1.028|66|0.0|55.951|0.000|0.000|0.000|32.810|
|compare/wal_scalar_full_update/lin|2.752|0.997|1.018|66|104.0|56.066|0.271|1.496|41.649|0.000|
|compare/wal_scalar_normal_update/lin|0.322|1.005|1.014|66|104.0|97.923|0.448|0.869|0.000|0.000|
