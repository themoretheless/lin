# Compatible column Batch WAL component benchmark

864 processes / 6912 observations. Diagnostic component; whole-engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/durable_update_1row_1k/lin|series1|7.739|2.399|6/6|True|
|compare/durable_update_1row_1k/lin|series2|-2.764|3.420|2/6|True|
|compare/durable_delete_1row_1k/lin|series1|2.581|2.288|4/6|True|
|compare/durable_delete_1row_1k/lin|series2|1.332|5.674|3/6|True|
|compare/durable_commit_1row_1k/lin|series1|-3.316|5.820|1/6|True|
|compare/durable_commit_1row_1k/lin|series2|-1.098|4.852|2/6|True|
|compare/durable_update_1row_10k/lin|series1|-3.373|5.278|1/6|True|
|compare/durable_update_1row_10k/lin|series2|0.144|3.746|3/6|True|
|compare/durable_delete_1row_10k/lin|series1|4.067|4.248|4/6|True|
|compare/durable_delete_1row_10k/lin|series2|-0.565|9.742|3/6|True|
|compare/durable_commit_1row_10k/lin|series1|-5.294|2.422|2/6|False|
|compare/durable_commit_1row_10k/lin|series2|-0.900|6.777|3/6|True|
|compare/wal_scalar_full_update/lin|series1|2.607|5.034|4/6|True|
|compare/wal_scalar_full_update/lin|series2|-2.152|15.047|2/6|True|
|compare/durable_update_1row_1k/sqlite_wal|series1|-0.422|3.412|3/6|True|
|compare/durable_update_1row_1k/sqlite_wal|series2|-4.743|11.658|2/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series1|0.700|3.986|3/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series2|1.186|3.315|5/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series1|0.143|2.994|4/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series2|0.512|4.395|3/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series1|-0.126|5.146|3/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series2|4.364|5.843|4/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series1|5.248|4.976|4/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series2|4.581|4.983|4/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series1|3.704|7.806|5/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series2|1.587|3.694|4/6|True|
|compare/wal_scalar_normal_update/lin|series1|2.213|14.956|4/6|True|
|compare/wal_scalar_normal_update/lin|series2|1.503|2.676|5/6|True|
|compare/update_1row_1k/lin|series1|-0.521|4.913|2/6|True|
|compare/update_1row_1k/lin|series2|-1.300|6.061|2/6|True|
|compare/update_1row_10k/lin|series1|-2.868|5.155|2/6|True|
|compare/update_1row_10k/lin|series2|-2.686|3.168|1/6|True|
|compare/insert_bulk_1k/lin|series1|-0.054|2.044|3/6|True|
|compare/insert_bulk_1k/lin|series2|-0.019|1.253|3/6|True|
|compare/insert_bulk_10k/lin|series1|-2.047|2.448|2/6|True|
|compare/insert_bulk_10k/lin|series2|0.922|1.210|4/6|True|
|compare/tx_rollback_1row_1k/lin|series1|3.045|3.173|4/6|True|
|compare/tx_rollback_1row_1k/lin|series2|0.362|3.693|3/6|True|
|compare/fts_lex_selective/lin|series1|-1.809|2.074|1/6|True|
|compare/fts_lex_selective/lin|series2|-3.008|3.207|1/6|True|
|compare/column_batch_full_insert_1k/lin|series1|81.702|6.416|6/6|True|
|compare/column_batch_full_insert_1k/lin|series2|82.589|4.796|6/6|True|
|compare/column_batch_full_insert_1k/sqlite_wal|series1|0.823|0.580|4/6|True|
|compare/column_batch_full_insert_1k/sqlite_wal|series2|0.494|1.179|3/6|True|
|compare/column_batch_full_insert_2k/lin|series1|78.780|4.853|6/6|True|
|compare/column_batch_full_insert_2k/lin|series2|80.221|14.847|6/6|True|
|compare/column_batch_full_insert_2k/sqlite_wal|series1|0.578|1.378|6/6|True|
|compare/column_batch_full_insert_2k/sqlite_wal|series2|0.363|1.175|3/6|True|
