# Columnar Batch candidate / SQLite WAL FULL

Original complete matrix, separate fresh processes. Ratio < 1 is a numeric Lin win. New grouped inserts and existing single-row writes retain their own declared workloads. No whole-engine or eight-peer qualification.

|Operation|Rows|Lin/SQLite median paired ratio|Numeric wins|
|---|---|---:|---:|
|update|1k|1.203414|0/12|
|delete|1k|1.239424|0/12|
|commit|1k|1.189467|0/12|
|update|10k|1.885262|0/12|
|delete|10k|1.880683|0/12|
|commit|10k|1.782941|0/12|
|column_batch_full_insert|1k|1.496603|0/12|
|column_batch_full_insert|2k|1.869497|0/12|
