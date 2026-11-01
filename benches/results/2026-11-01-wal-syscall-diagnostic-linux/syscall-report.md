# Lin / SQLite WAL FULL syscall observations

Single fresh invocation per case. Setup and close/reopen are outside markers. Traced execution is not a performance comparison. Counts and bytes do not establish crash safety or explain latency by themselves.

|Case|Write calls|Successful write bytes|fdatasync|fsync|
|---|---:|---:|---:|---:|
|1000-update-lin|1|4125|1|0|
|1000-update-sqlite_wal|6|12360|0|1|
|1000-delete-lin|1|4125|1|0|
|1000-delete-sqlite_wal|8|16480|0|1|
|1000-commit-lin|1|4125|1|0|
|1000-commit-sqlite_wal|6|12360|0|1|
|1000-rollback-lin|0|0|0|0|
|1000-rollback-sqlite_wal|0|0|0|0|
|10000-update-lin|1|4125|1|0|
|10000-update-sqlite_wal|6|12360|0|1|
|10000-delete-lin|1|4125|1|0|
|10000-delete-sqlite_wal|8|16480|0|1|
|10000-commit-lin|1|4125|1|0|
|10000-commit-sqlite_wal|6|12360|0|1|
|10000-rollback-lin|0|0|0|0|
|10000-rollback-sqlite_wal|0|0|0|0|
