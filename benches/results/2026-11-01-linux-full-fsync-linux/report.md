# Linux Full fsync component benchmark

720 processes / 5760 observations. Diagnostic component; whole-engine and eight-peer objective remain unqualified.

|Case|Series|Gain %|A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/durable_update_1row_1k/lin|series1|0.076|11.661|4/6|False|
|compare/durable_update_1row_1k/lin|series2|2.708|7.870|5/6|False|
|compare/durable_delete_1row_1k/lin|series1|-1.360|5.239|3/6|False|
|compare/durable_delete_1row_1k/lin|series2|-6.571|7.907|2/6|False|
|compare/durable_commit_1row_1k/lin|series1|-2.416|8.987|1/6|False|
|compare/durable_commit_1row_1k/lin|series2|-2.089|6.200|2/6|False|
|compare/durable_update_1row_10k/lin|series1|-5.416|5.739|1/6|False|
|compare/durable_update_1row_10k/lin|series2|2.952|2.141|5/6|False|
|compare/durable_delete_1row_10k/lin|series1|-2.312|4.816|2/6|False|
|compare/durable_delete_1row_10k/lin|series2|-2.359|3.479|2/6|False|
|compare/durable_commit_1row_10k/lin|series1|-4.021|3.565|2/6|False|
|compare/durable_commit_1row_10k/lin|series2|1.459|3.683|3/6|False|
|compare/wal_scalar_full_update/lin|series1|-0.391|1.459|3/6|False|
|compare/wal_scalar_full_update/lin|series2|2.901|8.052|4/6|False|
|compare/durable_update_1row_1k/sqlite_wal|series1|2.792|6.355|4/6|True|
|compare/durable_update_1row_1k/sqlite_wal|series2|3.391|2.151|5/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series1|-1.493|6.594|2/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series2|-3.010|4.807|2/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series1|-3.348|4.417|2/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series2|5.434|6.852|4/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series1|3.018|4.084|6/6|True|
|compare/durable_update_1row_10k/sqlite_wal|series2|-1.328|3.604|2/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series1|4.618|5.331|4/6|True|
|compare/durable_delete_1row_10k/sqlite_wal|series2|1.603|5.348|4/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series1|-0.510|2.912|2/6|True|
|compare/durable_commit_1row_10k/sqlite_wal|series2|-3.015|4.694|2/6|True|
|compare/wal_scalar_normal_update/lin|series1|1.725|1.989|4/6|True|
|compare/wal_scalar_normal_update/lin|series2|2.035|1.962|5/6|True|
|compare/update_1row_1k/lin|series1|1.283|3.141|5/6|True|
|compare/update_1row_1k/lin|series2|-2.258|3.561|0/6|True|
|compare/update_1row_10k/lin|series1|0.468|2.550|3/6|True|
|compare/update_1row_10k/lin|series2|-3.703|5.498|2/6|True|
|compare/insert_bulk_1k/lin|series1|0.498|0.902|5/6|True|
|compare/insert_bulk_1k/lin|series2|-0.908|1.780|2/6|True|
|compare/insert_bulk_10k/lin|series1|-1.506|1.234|0/6|True|
|compare/insert_bulk_10k/lin|series2|0.310|1.024|4/6|True|
|compare/tx_rollback_1row_1k/lin|series1|1.650|1.330|4/6|True|
|compare/tx_rollback_1row_1k/lin|series2|1.889|1.566|6/6|True|
|compare/fts_lex_selective/lin|series1|-1.333|1.989|2/6|True|
|compare/fts_lex_selective/lin|series2|-0.333|2.040|2/6|True|
