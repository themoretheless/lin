# Full DELETE WAL key projection component comparison

Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus Full DELETE WAL projection candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/durable_delete_1row_1k/lin|series1|11.672|4.748|5/6|False|
|compare/durable_delete_1row_1k/lin|series2|20.520|3.458|6/6|True|
|compare/durable_delete_1row_10k/lin|series1|9.401|3.043|5/6|False|
|compare/durable_delete_1row_10k/lin|series2|17.343|4.668|6/6|True|
|compare/durable_update_1row_1k/lin|series1|-0.569|5.214|3/6|True|
|compare/durable_update_1row_1k/lin|series2|4.297|4.077|6/6|True|
|compare/durable_commit_1row_10k/lin|series1|0.688|2.987|4/6|True|
|compare/durable_commit_1row_10k/lin|series2|5.662|6.294|5/6|True|
|compare/durable_rollback_1row_1k/lin|series1|-1.453|3.249|2/6|True|
|compare/durable_rollback_1row_1k/lin|series2|-2.358|1.688|2/6|False|
|compare/wal_scalar_full_update/lin|series1|3.957|2.963|5/6|True|
|compare/wal_scalar_full_update/lin|series2|1.027|4.361|5/6|True|
|compare/wal_scalar_normal_update/lin|series1|-0.815|1.809|0/6|True|
|compare/wal_scalar_normal_update/lin|series2|-0.375|0.806|2/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series1|1.665|5.042|4/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series2|1.122|2.272|4/6|True|

Component gate: False. All 288 processes / 2304 wall observations retained.
