# Rollback index upsert component comparison

Linux x86_64, same frozen compare harness; corrected COW+CAS+UPDATE baseline625bc7180 versus owned rollback index upsert candidate; only the explicit pre-delete of the rollback index slot is removed. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/tx_rollback_1row_1k/lin|series1|0.470|5.736|3/6|False|
|compare/tx_rollback_1row_1k/lin|series2|-1.872|12.720|1/6|False|
|compare/tx_rollback_1row_10k/lin|series1|5.288|2.290|6/6|True|
|compare/tx_rollback_1row_10k/lin|series2|0.780|2.434|4/6|False|
|compare/tx_rollback_1row_100k/lin|series1|1.199|1.185|4/6|False|
|compare/tx_rollback_1row_100k/lin|series2|1.827|1.052|4/6|False|
|compare/durable_rollback_1row_1k/lin|series1|0.035|1.678|3/6|False|
|compare/durable_rollback_1row_1k/lin|series2|3.860|6.408|5/6|False|
|compare/durable_rollback_1row_10k/lin|series1|-0.551|2.905|2/6|False|
|compare/durable_rollback_1row_10k/lin|series2|1.782|2.122|5/6|False|
|compare/update_1row_1k/lin|series1|-5.536|3.423|1/6|False|
|compare/update_1row_1k/lin|series2|6.281|7.181|4/6|True|
|compare/update_1row_10k/lin|series1|4.415|8.140|5/6|True|
|compare/update_1row_10k/lin|series2|1.484|4.971|5/6|True|
|compare/durable_update_1row_1k/lin|series1|0.760|11.566|3/6|True|
|compare/durable_update_1row_1k/lin|series2|-2.685|4.113|2/6|True|
|compare/durable_commit_1row_10k/lin|series1|1.483|4.784|5/6|True|
|compare/durable_commit_1row_10k/lin|series2|-0.025|2.518|3/6|True|
|compare/durable_delete_1row_1k/lin|series1|-2.414|4.174|2/6|True|
|compare/durable_delete_1row_1k/lin|series2|0.606|8.612|4/6|True|
|compare/wal_scalar_full_update/lin|series1|-2.709|8.721|3/6|True|
|compare/wal_scalar_full_update/lin|series2|2.026|3.021|4/6|True|
|compare/wal_scalar_normal_update/lin|series1|1.009|8.499|3/6|True|
|compare/wal_scalar_normal_update/lin|series2|-1.690|7.802|3/6|True|
|compare/durable_rollback_1row_1k/sqlite_wal|series1|-0.984|2.941|2/6|True|
|compare/durable_rollback_1row_1k/sqlite_wal|series2|-0.705|0.794|2/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series1|3.179|5.798|4/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series2|2.438|4.539|4/6|True|

Component gate: False. All 504 processes / 4032 wall observations retained.
