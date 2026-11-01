# Inline FTS tokens component benchmark

756 processes / 6048 observations. Diagnostic component; whole-engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|short|compare/tx_rollback_1row_1k/lin|series1|2.159|4.064|4/6|False|
|short|compare/tx_rollback_1row_1k/lin|series2|0.479|2.678|4/6|False|
|short|compare/tx_rollback_1row_10k/lin|series1|2.726|2.205|6/6|True|
|short|compare/tx_rollback_1row_10k/lin|series2|1.931|1.254|5/6|False|
|short|compare/tx_rollback_1row_100k/lin|series1|1.975|1.872|5/6|False|
|short|compare/tx_rollback_1row_100k/lin|series2|0.533|2.516|4/6|False|
|short|compare/insert_bulk_1k/lin|series1|-2.689|2.681|2/6|False|
|short|compare/insert_bulk_1k/lin|series2|1.704|1.448|4/6|True|
|short|compare/insert_bulk_10k/lin|series1|0.589|2.637|3/6|True|
|short|compare/insert_bulk_10k/lin|series2|-2.171|1.765|2/6|False|
|short|compare/update_1row_1k/lin|series1|-0.998|2.249|3/6|True|
|short|compare/update_1row_1k/lin|series2|-0.198|1.779|3/6|True|
|short|compare/update_1row_10k/lin|series1|3.253|1.027|6/6|True|
|short|compare/update_1row_10k/lin|series2|6.414|2.762|6/6|True|
|short|compare/delete_1row_1k/lin|series1|-4.017|2.865|2/6|False|
|short|compare/delete_1row_1k/lin|series2|1.271|3.778|4/6|True|
|short|compare/delete_1row_10k/lin|series1|-0.450|1.796|3/6|True|
|short|compare/delete_1row_10k/lin|series2|-2.443|1.547|1/6|False|
|short|compare/tx_commit_1row_10k/lin|series1|4.182|1.585|5/6|True|
|short|compare/tx_commit_1row_10k/lin|series2|7.257|4.222|5/6|True|
|short|compare/durable_update_1row_1k/lin|series1|19.342|9.429|6/6|True|
|short|compare/durable_update_1row_1k/lin|series2|1.484|12.030|3/6|True|
|short|compare/wal_scalar_full_update/lin|series1|-0.778|4.536|3/6|True|
|short|compare/wal_scalar_full_update/lin|series2|2.375|6.094|4/6|True|
|short|compare/wal_scalar_normal_update/lin|series1|4.708|2.954|4/6|True|
|short|compare/wal_scalar_normal_update/lin|series2|1.035|2.888|4/6|True|
|short|compare/fts_lex_selective/lin|series1|-4.778|4.944|2/6|True|
|short|compare/fts_lex_selective/lin|series2|-2.874|2.506|1/6|False|
|short|compare/fts_lex_common/lin|series1|4.434|1.939|6/6|True|
|short|compare/fts_lex_common/lin|series2|1.711|1.948|6/6|True|
|short|compare/filter_eq/duckdb|series1|-1.147|6.098|2/6|True|
|short|compare/filter_eq/duckdb|series2|-4.747|6.473|2/6|True|
|long|compare/tx_rollback_1row_1k/lin|series1|-2.409|2.229|2/6|False|
|long|compare/tx_rollback_1row_1k/lin|series2|0.628|2.673|3/6|True|
|long|compare/tx_rollback_1row_10k/lin|series1|-1.239|2.521|1/6|True|
|long|compare/tx_rollback_1row_10k/lin|series2|-0.442|2.970|2/6|True|
|long|compare/update_1row_1k/lin|series1|1.714|3.754|4/6|True|
|long|compare/update_1row_1k/lin|series2|-5.212|8.257|2/6|True|
|long|compare/delete_1row_1k/lin|series1|-0.641|1.874|3/6|True|
|long|compare/delete_1row_1k/lin|series2|-4.822|3.387|0/6|False|
|long|compare/tx_commit_1row_1k/lin|series1|1.372|1.645|5/6|True|
|long|compare/tx_commit_1row_1k/lin|series2|1.526|6.243|3/6|True|
