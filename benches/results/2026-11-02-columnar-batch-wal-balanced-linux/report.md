# Compatible column Batch WAL component benchmark

864 processes / 6912 observations. Diagnostic component; whole-engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/durable_update_1row_1k/lin|series1|3.681|4.253|4/6|True|
|compare/durable_update_1row_1k/lin|series2|-2.004|6.622|2/6|True|
|compare/durable_delete_1row_1k/lin|series1|-1.777|7.935|2/6|True|
|compare/durable_delete_1row_1k/lin|series2|-1.624|5.413|2/6|True|
|compare/durable_commit_1row_1k/lin|series1|4.319|2.078|5/6|True|
|compare/durable_commit_1row_1k/lin|series2|-2.774|7.117|1/6|True|
|compare/durable_update_1row_10k/lin|series1|-3.882|3.564|1/6|False|
|compare/durable_update_1row_10k/lin|series2|-2.085|5.147|2/6|True|
|compare/durable_delete_1row_10k/lin|series1|1.583|1.239|4/6|True|
|compare/durable_delete_1row_10k/lin|series2|-3.064|6.842|2/6|True|
|compare/durable_commit_1row_10k/lin|series1|-1.898|6.030|2/6|True|
|compare/durable_commit_1row_10k/lin|series2|-0.027|6.840|3/6|True|
|compare/wal_scalar_full_update/lin|series1|-4.850|7.056|2/6|True|
|compare/wal_scalar_full_update/lin|series2|-4.884|8.083|2/6|True|
|compare/durable_update_1row_1k/sqlite_wal|series1|-3.749|5.321|2/6|True|
|compare/durable_update_1row_1k/sqlite_wal|series2|-1.214|4.169|2/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series1|1.728|4.022|5/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series2|2.350|4.721|4/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series1|-1.984|6.510|2/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series2|-0.255|5.869|3/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series1|4.998|2.231|4/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series2|0.450|4.279|3/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series1|-2.433|6.850|3/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series2|-1.037|6.114|3/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series1|-2.914|5.589|1/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series2|2.729|2.045|4/6|True|
|compare/wal_scalar_normal_update/lin|series1|2.200|2.909|5/6|True|
|compare/wal_scalar_normal_update/lin|series2|2.339|1.985|4/6|True|
|compare/update_1row_1k/lin|series1|-1.935|4.449|2/6|True|
|compare/update_1row_1k/lin|series2|-3.019|6.398|0/6|True|
|compare/update_1row_10k/lin|series1|2.618|2.337|4/6|True|
|compare/update_1row_10k/lin|series2|-5.234|2.665|2/6|False|
|compare/insert_bulk_1k/lin|series1|-0.266|0.777|2/6|True|
|compare/insert_bulk_1k/lin|series2|-0.715|0.526|1/6|True|
|compare/insert_bulk_10k/lin|series1|-0.942|0.522|1/6|True|
|compare/insert_bulk_10k/lin|series2|-0.422|1.086|2/6|True|
|compare/tx_rollback_1row_1k/lin|series1|0.818|2.922|3/6|True|
|compare/tx_rollback_1row_1k/lin|series2|1.584|1.817|5/6|True|
|compare/fts_lex_selective/lin|series1|-2.111|3.302|2/6|True|
|compare/fts_lex_selective/lin|series2|-1.840|3.421|1/6|True|
|compare/column_batch_full_insert_1k/lin|series1|82.467|3.618|6/6|True|
|compare/column_batch_full_insert_1k/lin|series2|82.400|3.122|6/6|True|
|compare/column_batch_full_insert_1k/sqlite_wal|series1|-0.757|0.895|2/6|True|
|compare/column_batch_full_insert_1k/sqlite_wal|series2|-0.344|1.302|2/6|True|
|compare/column_batch_full_insert_2k/lin|series1|79.070|1.372|6/6|True|
|compare/column_batch_full_insert_2k/lin|series2|79.209|1.810|6/6|True|
|compare/column_batch_full_insert_2k/sqlite_wal|series1|-0.947|0.669|1/6|True|
|compare/column_batch_full_insert_2k/sqlite_wal|series2|0.197|1.363|5/6|True|
