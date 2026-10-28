# Bulk owned-map template: rejected

Run40616712983,frozen84817f211. Independent source/raw audit passed for288fresh processes/2304observations;449Linux release tests/2ignored and16fresh correctness smokes passed. All4bulk INSERT target windows and2UPDATEcontrol windows failed. No production adoption or unchanged rerun.

|Case|Series|Gain %|Absolute A/A %|Positive pairs|Pass|
|---|---|---:|---:|---:|---|
|compare/insert_bulk_1k/lin|series1|0.7245|1.6278|3/6|False|
|compare/insert_bulk_1k/lin|series2|-1.4999|1.3610|2/6|False|
|compare/insert_bulk_10k/lin|series1|-0.9877|2.2949|2/6|False|
|compare/insert_bulk_10k/lin|series2|0.1296|0.7507|4/6|False|
|compare/filter_eq/lin|series1|-0.6239|3.2600|3/6|True|
|compare/filter_eq/lin|series2|-2.1513|3.8370|1/6|True|
|compare/update_1row_1k/lin|series1|-1.2562|1.5106|2/6|True|
|compare/update_1row_1k/lin|series2|-2.0148|1.9357|2/6|False|
|compare/update_1row_100k/lin|series1|1.4537|3.0706|5/6|True|
|compare/update_1row_100k/lin|series2|-3.4760|1.2729|1/6|False|
|compare/wal_scalar_full_update/lin|series1|1.3406|7.1970|3/6|True|
|compare/wal_scalar_full_update/lin|series2|-0.7712|4.6722|3/6|True|
|compare/wal_scalar_normal_update/lin|series1|1.2823|1.1927|5/6|True|
|compare/wal_scalar_normal_update/lin|series2|-1.2107|1.0034|1/6|True|
|compare/insert_bulk_10k/sqlite|series1|0.4742|0.5491|5/6|True|
|compare/insert_bulk_10k/sqlite|series2|0.0731|0.3762|4/6|True|

INSERT1k changes+0.7245%/-1.4999%,INSERT10k-0.9877%/+0.1296% failthe6/6andA/A criteria. UPDATE1k/100k controls inseries2 fail-2.0148%/-3.4760%. No unique causal explanation is established. This experiment does not establish COW enum dispatch as the cause of the prior bulk regression.

The current DELETE-WAL candidate retains the preceding Row-based bulk template and excludes this hypothesis. The experimental branch and exact candidate source remain archived. No inference from simpler source shape substitutes for measured acceptance. All failed pairs/startup observations,resource/build/test/smoke records remain in raw-artifact.tar.gz with every member hash verified. Full-source/eight-peer acceptance remains false.
