# Bulk owned-map template component comparison

Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus owned-map bulk template candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/insert_bulk_1k/lin|series1|0.724|1.628|3/6|False|
|compare/insert_bulk_1k/lin|series2|-1.500|1.361|2/6|False|
|compare/insert_bulk_10k/lin|series1|-0.988|2.295|2/6|False|
|compare/insert_bulk_10k/lin|series2|0.130|0.751|4/6|False|
|compare/filter_eq/lin|series1|-0.624|3.260|3/6|True|
|compare/filter_eq/lin|series2|-2.151|3.837|1/6|True|
|compare/update_1row_1k/lin|series1|-1.256|1.511|2/6|True|
|compare/update_1row_1k/lin|series2|-2.015|1.936|2/6|False|
|compare/update_1row_100k/lin|series1|1.454|3.071|5/6|True|
|compare/update_1row_100k/lin|series2|-3.476|1.273|1/6|False|
|compare/wal_scalar_full_update/lin|series1|1.341|7.197|3/6|True|
|compare/wal_scalar_full_update/lin|series2|-0.771|4.672|3/6|True|
|compare/wal_scalar_normal_update/lin|series1|1.282|1.193|5/6|True|
|compare/wal_scalar_normal_update/lin|series2|-1.211|1.003|1/6|True|
|compare/insert_bulk_10k/sqlite|series1|0.474|0.549|5/6|True|
|compare/insert_bulk_10k/sqlite|series2|0.073|0.376|4/6|True|

Component gate: False. All 288 processes / 2304 wall observations retained.
