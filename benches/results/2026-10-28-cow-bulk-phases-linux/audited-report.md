# Native bulk phase diagnostic

All48fresh processes/384wall observations retained. Original/control share one binary;disabled/traced share an instrumented binary. Phase shares are medians within each process then across processes;validation/calibration invocations are included andnot aligned1:1withmeasured observations. Clock/trace overhead is recorded,not hidden. No latency,source acceptance,RSS orpeer victory is qualified.

|Case|A/A %|Disabled/original|Traced/disabled|Trace invocations|
|---|---:|---:|---:|---:|
|compare/insert_bulk_1k/lin|0.3548|0.9971|0.9984|66|
|compare/insert_bulk_10k/lin|1.9688|1.0006|1.0010|66|

|Case|Phase|Caller wall %|
|---|---|---:|
|compare/insert_bulk_1k/lin|row_build|15.5648|
|compare/insert_bulk_1k/lin|id_hash_fk|10.5357|
|compare/insert_bulk_1k/lin|embedding|32.2860|
|compare/insert_bulk_1k/lin|edges|0.0093|
|compare/insert_bulk_1k/lin|indexes_reserve|21.2029|
|compare/insert_bulk_1k/lin|fts|14.2175|
|compare/insert_bulk_1k/lin|hnsw|0.0044|
|compare/insert_bulk_1k/lin|row_maps|5.7926|
|compare/insert_bulk_1k/lin|store_install|0.0797|
|compare/insert_bulk_1k/lin|unattributed|0.1570|
|compare/insert_bulk_10k/lin|row_build|11.1896|
|compare/insert_bulk_10k/lin|id_hash_fk|12.5948|
|compare/insert_bulk_10k/lin|embedding|40.6656|
|compare/insert_bulk_10k/lin|edges|0.0022|
|compare/insert_bulk_10k/lin|indexes_reserve|12.1856|
|compare/insert_bulk_10k/lin|fts|15.4642|
|compare/insert_bulk_10k/lin|hnsw|0.0013|
|compare/insert_bulk_10k/lin|row_maps|7.3572|
|compare/insert_bulk_10k/lin|store_install|0.0813|
|compare/insert_bulk_10k/lin|unattributed|0.0407|

Independent medians need not sumto100%. row_build includes construction workers andjoin. Input Db teardown andsetup are excluded. payload_bytes is unused by this CPU-only instrument andmust not be interpreted as a WAL measurement.
