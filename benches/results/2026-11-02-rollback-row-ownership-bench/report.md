# COW row ownership local rollback discovery

Same exact harness and workloads, baseline60e64559e versus ownership candidate408f02f70. macOS ARM64; local host activity uncontrolled. All external roles share one identical baseline executable. This is not Linux qualification or an external superiority claim. Five rollback targets and 15 controls, unchanged thresholds. Whole engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/tx_rollback_1row_1k/lin|series1|-14.024|13.523|2/6|False|
|compare/tx_rollback_1row_1k/lin|series2|-0.026|13.391|3/6|False|
|compare/tx_rollback_1row_10k/lin|series1|0.742|5.830|4/6|False|
|compare/tx_rollback_1row_10k/lin|series2|-1.565|5.329|2/6|False|
|compare/tx_rollback_1row_100k/lin|series1|-2.699|7.677|1/6|False|
|compare/tx_rollback_1row_100k/lin|series2|-1.508|2.615|2/6|False|
|compare/durable_rollback_1row_1k/lin|series1|1.125|5.969|3/6|False|
|compare/durable_rollback_1row_1k/lin|series2|3.432|6.338|4/6|False|
|compare/durable_rollback_1row_10k/lin|series1|4.958|4.951|4/6|False|
|compare/durable_rollback_1row_10k/lin|series2|6.653|2.872|6/6|True|
|compare/update_1row_1k/lin|series1|8.422|9.455|5/6|True|
|compare/update_1row_1k/lin|series2|-38.956|22.834|2/6|False|
|compare/update_1row_10k/lin|series1|4.301|5.475|3/6|True|
|compare/update_1row_10k/lin|series2|2.028|4.165|5/6|True|
|compare/durable_update_1row_1k/lin|series1|-4.349|3.248|0/6|False|
|compare/durable_update_1row_1k/lin|series2|0.393|6.779|4/6|True|
|compare/durable_commit_1row_10k/lin|series1|1.204|14.247|3/6|True|
|compare/durable_commit_1row_10k/lin|series2|1.138|6.171|4/6|True|
|compare/durable_delete_1row_1k/lin|series1|-2.986|2.220|2/6|False|
|compare/durable_delete_1row_1k/lin|series2|0.491|10.477|5/6|True|
|compare/wal_scalar_full_update/lin|series1|0.957|2.183|3/6|True|
|compare/wal_scalar_full_update/lin|series2|-3.121|3.757|2/6|True|
|compare/wal_scalar_normal_update/lin|series1|1.937|6.739|3/6|True|
|compare/wal_scalar_normal_update/lin|series2|-1.648|7.698|3/6|True|
|compare/durable_rollback_1row_1k/sqlite_wal|series1|6.225|6.694|4/6|True|
|compare/durable_rollback_1row_1k/sqlite_wal|series2|3.992|11.360|5/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series1|-6.114|7.116|1/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series2|-2.964|13.987|2/6|True|
|compare/delete_1row_10k/lin|series1|-2.352|12.749|3/6|True|
|compare/delete_1row_10k/lin|series2|5.831|6.658|4/6|True|
|compare/insert_bulk_10k/lin|series1|5.938|4.625|4/6|True|
|compare/insert_bulk_10k/lin|series2|3.183|3.890|5/6|True|
|compare/fts_lex_selective/lin|series1|-1.533|7.007|3/6|True|
|compare/fts_lex_selective/lin|series2|7.645|9.697|5/6|True|
|compare/agg_count_by/duckdb|series1|-5.461|8.922|2/6|True|
|compare/agg_count_by/duckdb|series2|4.024|4.347|4/6|True|
|compare/durable_delete_1row_10k/sqlite_delete|series1|-5.755|5.862|0/6|True|
|compare/durable_delete_1row_10k/sqlite_delete|series2|-0.983|4.535|3/6|True|
|compare/durable_commit_1row_1k/lin|series1|-3.451|4.319|2/6|True|
|compare/durable_commit_1row_1k/lin|series2|0.371|6.784|3/6|True|

Component gate: False. All 720 processes / 5760 wall observations retained.
