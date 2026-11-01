# Columnar Batch candidate / SQLite WAL FULL

Original complete matrix, separate fresh processes. Ratio < 1 is a numeric Lin win. New grouped inserts and existing single-row writes retain their own declared workloads. No whole-engine or eight-peer qualification.

|Operation|Rows|Lin/SQLite median paired ratio|Numeric wins|
|---|---|---:|---:|
|update|1k|1.273173|0/12|
|delete|1k|1.272971|0/12|
|commit|1k|1.368873|0/12|
|update|10k|1.701483|1/12|
|delete|10k|1.818862|1/12|
|commit|10k|1.765255|0/12|
|column_batch_full_insert|1k|1.530773|0/12|
|column_batch_full_insert|2k|1.867728|0/12|
