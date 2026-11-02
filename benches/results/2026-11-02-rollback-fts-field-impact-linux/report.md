# Rollback FTS journal field-impact component comparison

Same exact harness and workloads, baseline625bc7180 versus FTS field-impact candidate65bc82590. Five rollback targets and 15 controls, unchanged thresholds. Whole engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/tx_rollback_1row_1k/lin|series1|27.311|0.699|6/6|True|
|compare/tx_rollback_1row_1k/lin|series2|26.743|1.836|6/6|True|
|compare/tx_rollback_1row_10k/lin|series1|23.499|2.572|6/6|True|
|compare/tx_rollback_1row_10k/lin|series2|23.771|1.653|6/6|True|
|compare/tx_rollback_1row_100k/lin|series1|24.223|6.146|6/6|True|
|compare/tx_rollback_1row_100k/lin|series2|23.464|3.604|6/6|True|
|compare/durable_rollback_1row_1k/lin|series1|26.207|1.419|6/6|True|
|compare/durable_rollback_1row_1k/lin|series2|24.320|4.148|6/6|True|
|compare/durable_rollback_1row_10k/lin|series1|19.862|2.664|6/6|True|
|compare/durable_rollback_1row_10k/lin|series2|20.110|2.184|6/6|True|
|compare/update_1row_1k/lin|series1|-1.475|2.712|1/6|True|
|compare/update_1row_1k/lin|series2|4.731|1.715|4/6|True|
|compare/update_1row_10k/lin|series1|-2.943|2.757|2/6|False|
|compare/update_1row_10k/lin|series2|2.830|2.835|4/6|True|
|compare/durable_update_1row_1k/lin|series1|2.168|8.011|5/6|True|
|compare/durable_update_1row_1k/lin|series2|3.608|3.829|4/6|True|
|compare/durable_commit_1row_10k/lin|series1|2.903|5.546|5/6|True|
|compare/durable_commit_1row_10k/lin|series2|2.255|6.708|3/6|True|
|compare/durable_delete_1row_1k/lin|series1|1.498|6.130|3/6|True|
|compare/durable_delete_1row_1k/lin|series2|-4.884|2.228|0/6|False|
|compare/wal_scalar_full_update/lin|series1|5.898|5.764|5/6|True|
|compare/wal_scalar_full_update/lin|series2|-10.830|5.226|2/6|False|
|compare/wal_scalar_normal_update/lin|series1|0.321|2.905|4/6|True|
|compare/wal_scalar_normal_update/lin|series2|-2.479|2.393|2/6|False|
|compare/durable_rollback_1row_1k/sqlite_wal|series1|0.443|0.818|4/6|True|
|compare/durable_rollback_1row_1k/sqlite_wal|series2|-1.159|1.062|1/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series1|0.085|1.093|3/6|True|
|compare/durable_rollback_1row_10k/sqlite_wal|series2|-0.276|2.806|2/6|True|
|compare/delete_1row_10k/lin|series1|-2.130|2.915|2/6|True|
|compare/delete_1row_10k/lin|series2|-3.096|2.433|1/6|False|
|compare/insert_bulk_10k/lin|series1|-4.580|4.380|2/6|False|
|compare/insert_bulk_10k/lin|series2|-1.979|2.046|2/6|True|
|compare/fts_lex_selective/lin|series1|2.515|1.795|5/6|True|
|compare/fts_lex_selective/lin|series2|0.846|2.005|4/6|True|
|compare/agg_count_by/duckdb|series1|-14.757|5.880|2/6|False|
|compare/agg_count_by/duckdb|series2|-7.031|3.715|3/6|False|
|compare/durable_delete_1row_10k/sqlite_delete|series1|2.241|4.108|4/6|True|
|compare/durable_delete_1row_10k/sqlite_delete|series2|-0.445|3.500|3/6|True|
|compare/durable_commit_1row_1k/lin|series1|-0.548|4.083|3/6|True|
|compare/durable_commit_1row_1k/lin|series2|-1.244|3.622|2/6|True|

Component gate: False. All 720 processes / 5760 wall observations retained.
