# Setup WAL history from retained raw traces

These counts precede the mutation begin marker and are excluded from measured mutation scope. They describe successful WAL writes/syncs, not file allocation, physical storage traffic, latency, metadata size or crash safety. Full traces and their SHA-256 are retained. All four operations at each engine/size have identical setup counts.

|Rows|Engine|WAL sync calls|WAL write calls|Successful write bytes|
|---:|---|---:|---:|---:|
|1000|lin|4|3|337212|
|1000|sqlite_wal|2|69|140112|
|10000|lin|22|21|3560906|
|10000|sqlite_wal|2|595|1223672|

The frozen harness setup_lin_write_db inserts chunks of 500 through separate db.run calls. fill_sqlite_rows wraps all rows in one SQLite transaction. The schemas/storage layouts also differ; these counts do not isolate seed transaction count as a cause.

A future history-controlled diagnostic can vary the seeding transaction pattern while retaining original workloads and raw results. It must not reinterpret the old frozen comparison as passed. Current Linux fsync experiment keeps the original fixture unchanged.
