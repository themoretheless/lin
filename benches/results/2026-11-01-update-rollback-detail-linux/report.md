# UPDATE and rollback detail diagnostic

96 processes / 768 wall observations; traces include calibration and validation, without observation alignment. Ordinary/control and disabled/traced overhead comparisons retained. No engine or peer qualification.

|Case|A/A %|Disabled/original|Traced/disabled|positions|cas|effective_patch|guards_fk_undo|live_mutation|forward_indexes|forward_fts_hnsw_maps_output|rollback_delta|rollback_indexes|rollback_fts|rollback_maps|
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|compare/durable_update_1row_1k/lin|9.290|0.965|1.016|0.364|0.082|0.123|0.351|0.162|0.587|0.362|0.000|0.000|0.000|0.000|
|compare/durable_rollback_1row_1k/lin|0.587|1.054|1.076|6.506|1.331|2.185|5.720|2.747|9.896|6.686|5.347|8.180|24.691|2.832|
|compare/durable_update_1row_10k/lin|4.826|1.002|0.964|0.691|0.143|0.227|0.434|0.179|0.895|0.457|0.000|0.000|0.000|0.000|
|compare/durable_rollback_1row_10k/lin|1.547|1.081|1.042|9.130|1.727|2.984|6.307|2.420|12.337|6.514|4.815|5.572|18.982|1.546|
