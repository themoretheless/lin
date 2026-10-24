# Current native insertion: 15/15 aggregate wins

Source `e9206ad39b3c4f5af181ef512e78783c07916dbe`; frozen release worker and source hashes in environment.json. No production code changed for this refresh. This confirms the current source after bulk short-body hashing against five available database peers.

Three independent processes per size, rotated engine order, 12 fresh samples per engine/process. Exact six-field and multiplicity readback is outside timing; setup, schema/index creation, preparation, IPC and cleanup excluded. Lin default embeddings and FTS remain enabled. Default AST quotas retained; 100k uses ten prepared <=10k statements in one public memory transaction.

| Rows | Peer | Lin ms | Peer ms | Peer/Lin | Process wins |
|---:|---|---:|---:|---:|---:|
| 1000 | sqlite | 0.742250 | 1.008354 | 1.359 | 3/3 |
| 1000 | duckdb | 0.742250 | 1.149729 | 1.549 | 3/3 |
| 1000 | postgres | 0.742250 | 4.636812 | 6.247 | 3/3 |
| 1000 | mysql | 0.742250 | 8.261333 | 11.130 | 3/3 |
| 1000 | mongo | 0.742250 | 4.987000 | 6.719 | 3/3 |
| 10000 | sqlite | 7.940334 | 12.647563 | 1.593 | 3/3 |
| 10000 | duckdb | 7.940334 | 9.562313 | 1.204 | 3/3 |
| 10000 | postgres | 7.940334 | 26.673750 | 3.359 | 3/3 |
| 10000 | mysql | 7.940334 | 81.263126 | 10.234 | 3/3 |
| 10000 | mongo | 7.940334 | 47.475355 | 5.979 | 3/3 |
| 100000 | sqlite | 90.295666 | 143.292708 | 1.587 | 3/3 |
| 100000 | duckdb | 90.295666 | 99.004334 | 1.096 | 3/3 |
| 100000 | postgres | 90.295666 | 269.843458 | 2.988 | 3/3 |
| 100000 | mysql | 90.295666 | 849.534020 | 9.408 | 3/3 |
| 100000 | mongo | 90.295666 | 552.456250 | 6.118 | 3/3 |

## Scope and remaining work

SQLite executemany+commit; DuckDB prepared INSERT SELECT from registered pandas+commit; PostgreSQL COPY CSV+commit; MySQL multi-row executemany+commit; Mongo ordered insert_many w=1/j=false. APIs/driver transfer/features differ. This is a native memory API comparison, not equal disk durability. Native ARM64 task-owned servers use tmpfs; versions/settings/image digests captured. PostgreSQL sync/fsync/full-page-writes stayed on; MySQL flush/binlog settings 1/1.

Sentinel objects remained unchanged, no benchmark fixtures remained after each size, all three task-owned servers stopped. Source and benchmark hashes verified unchanged. All raw observations, including outliers, retained. The first runner invocation failed on a Python Path expression before starting containers; corrected and recorded separately.

The previous pre-hash source lost 100k to DuckDB (112.806 vs 101.463 ms); this fresh campaign is 90.296 vs 99.004 ms. Different campaigns are not a paired causal estimate; use the existing bulk-short-body-hash paired evidence for the optimization claim.

MSSQL and Kusto configuration is absent in this process environment; neither is counted as a win. Pandas construction has no equivalent insert_native contract in the adapter and remains separate from read comparisons. Durable 1k SQLite gap, current-source full read refresh and missing-peer qualification remain open. No whole-objective completion claim.
