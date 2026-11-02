# UPDATE patch clone elision component comparison

Linux x86_64, same frozen compare harness; corrected baseline21dcd7d81 versus patch clone elision; Full WAL encoding unchanged. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/update_1row_1k/lin|series1|46.902|16.579|6/6|True|
|compare/update_1row_1k/lin|series2|40.511|15.262|6/6|True|
|compare/update_1row_10k/lin|series1|32.295|1.318|6/6|True|
|compare/update_1row_10k/lin|series2|34.001|4.899|6/6|True|
|compare/update_1row_100k/lin|series1|37.177|4.815|6/6|True|
|compare/update_1row_100k/lin|series2|34.765|4.509|6/6|True|
|compare/durable_update_1row_1k/lin|series1|7.081|9.777|5/6|True|
|compare/durable_update_1row_1k/lin|series2|1.525|3.486|5/6|True|
|compare/durable_update_1row_10k/lin|series1|2.057|6.946|4/6|True|
|compare/durable_update_1row_10k/lin|series2|5.907|5.474|6/6|True|
|compare/durable_delete_1row_10k/lin|series1|4.106|2.193|5/6|True|
|compare/durable_delete_1row_10k/lin|series2|3.130|4.162|5/6|True|
|compare/durable_commit_1row_10k/lin|series1|1.198|2.694|4/6|True|
|compare/durable_commit_1row_10k/lin|series2|-3.739|5.135|2/6|True|
|compare/durable_rollback_1row_10k/lin|series1|1.917|1.410|5/6|True|
|compare/durable_rollback_1row_10k/lin|series2|1.799|2.669|4/6|True|
|compare/wal_scalar_full_update/lin|series1|3.068|3.551|5/6|True|
|compare/wal_scalar_full_update/lin|series2|2.186|4.933|5/6|True|
|compare/wal_scalar_normal_update/lin|series1|31.681|6.310|6/6|True|
|compare/wal_scalar_normal_update/lin|series2|33.397|7.629|6/6|True|
|compare/insert_bulk_10k/lin|series1|-1.245|3.026|2/6|True|
|compare/insert_bulk_10k/lin|series2|-0.246|1.896|2/6|True|
|compare/filter_eq/lin|series1|-0.028|2.219|3/6|True|
|compare/filter_eq/lin|series2|-0.264|3.280|3/6|True|
|compare/filter_eq/duckdb|series1|-11.684|8.412|2/6|False|
|compare/filter_eq/duckdb|series2|0.242|3.236|3/6|True|
|compare/update_1row_1k/sqlite|series1|1.404|6.628|3/6|True|
|compare/update_1row_1k/sqlite|series2|-2.161|3.612|2/6|True|

Component gate: False. All 504 processes / 4032 wall observations retained.
