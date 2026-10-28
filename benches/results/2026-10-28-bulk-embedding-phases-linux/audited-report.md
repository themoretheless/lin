# Bulk embedding phase diagnostic

All48fresh processes/384wall observations retained. Original/control share one binary;disabled/traced share an instrumented binary. Phase shares are medians within each process then across processes;validation/calibration invocations are included andnot aligned1:1withmeasured observations. Clock/trace overhead is recorded,not hidden. No latency,source acceptance,RSS orpeer victory is qualified.

|Case|A/A %|Disabled/original|Traced/disabled|Trace invocations|
|---|---:|---:|---:|---:|
|compare/insert_bulk_1k/lin|1.8693|1.0040|1.1811|66|
|compare/insert_bulk_10k/lin|3.1515|0.9985|1.1548|66|

|Case|Phase|Caller wall %|
|---|---|---:|
|compare/insert_bulk_1k/lin|text_collect|4.0178|
|compare/insert_bulk_1k/lin|refs|0.0353|
|compare/insert_bulk_1k/lin|lowercase|1.9858|
|compare/insert_bulk_1k/lin|feature_hash|7.9694|
|compare/insert_bulk_1k/lin|normalize|3.5240|
|compare/insert_bulk_1k/lin|vector_copy|8.9571|
|compare/insert_bulk_1k/lin|scratch_reset|2.0264|
|compare/insert_bulk_1k/lin|row_install|4.0896|
|compare/insert_bulk_1k/lin|unattributed|66.7508|
|compare/insert_bulk_10k/lin|text_collect|3.9060|
|compare/insert_bulk_10k/lin|refs|0.0366|
|compare/insert_bulk_10k/lin|lowercase|1.7813|
|compare/insert_bulk_10k/lin|feature_hash|7.3426|
|compare/insert_bulk_10k/lin|normalize|3.1980|
|compare/insert_bulk_10k/lin|vector_copy|15.3035|
|compare/insert_bulk_10k/lin|scratch_reset|1.8013|
|compare/insert_bulk_10k/lin|row_install|5.6967|
|compare/insert_bulk_10k/lin|unattributed|60.8905|

Independent medians need not sumto100%. Only embedding subphases are traced;unattributed includes the rest of bulk INSERT and observer overhead. Input Db teardown andsetup are excluded. payload_bytes is unused by this CPU-only instrument andmust not be interpreted as a WAL measurement.
