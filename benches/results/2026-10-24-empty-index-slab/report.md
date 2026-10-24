# Empty indexed slab panic — correctness regression

2026-10-24 baseline4f80a14. Public Store::index_insert_slab previously selected the single-row path for rows.len()==0 and indexed rows[0] whenever a matching index existed. A new regression first compiled successfully then failed at the actual out-of-bounds access (before-test.log, exit101). This is a reproduced correctness failure, not an optimization hypothesis or peer benchmark.

Fix: return Ok(()) immediately for an empty row slice, before label collection, row indexing or mutations. Nonempty indexing algorithms and the accepted sample-once bitmap optimization are unchanged. Empty slab is a no-op regardless of start or whether the collection has an index. No WAL/sync/public type/format change.

Regression covers empty and populated stores with an explicit docs wing/ts index; calls for docs/users/missing collections and starts0/current-row-count/usize::MAX,18 combinations. After each call compare index-map length, every forward posting map, reverse map and field list, plus exact original docs rows. Populated setup contains two docs. This is finite no-op/invariant coverage; not fault injection, arbitrary corrupted index repair, persistence replay or physical power-cut qualification.

Focused test and full default cargo test --workspace pass (checks.json and logs). Complete before/final store.rs snapshots, final source hashes and compiler version retained. No release benchmark run or new peer win claimed. The prior 108-read/15-native available-peer refresh belongs to the previously measured executable; this guard has not been separately timed. Invalid/zero-size performance workloads must not count as victories.

This closes a real boundary panic while preserving the full performance objective. Durable1kSQLite gap and missing live MSSQL/Kusto comparisons remain open; SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas superiority is not complete.
