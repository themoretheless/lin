# Prepared DuckDB native bulk insertion

Baseline source 5700a27 plus prepared SQL adapter. Lin production unchanged; pinned Lin worker from source 588136e, binary hash in binary-sha256.json. DuckDB 1.5.6, pandas input 3.0.6, Lin 0.4.0. Three independent process repetitions each size, 24 fresh samples each; exact six-field values/multiplicity checked outside timing. Both parent reports complete, no errors, require-wins exit zero. Nine contract tests pass. No builds/tests overlap timing.

| Rows | Lin median ms | DuckDB median ms | DuckDB / Lin | Lin process wins |
|---|---:|---:|---:|---:|
| 1000 | 1.377333 | 3.253458 | 2.362x | 3/3 |
| 10000 | 22.526917 | 25.043146 | 1.112x | 2/3 |

DuckDB now PREPAREs INSERT SELECT after registering the pandas input frame and before starting the timer. Timed operations are BEGIN TRANSACTION, EXECUTE insert_docs, commit. Schema/index/registration/preparation/readback/close excluded. Primary id, unique uri, wing/ts index and fresh :memory: fixture retained. Lin prepared Rust execution includes default embedding and FTS, excludes IPC. DuckDB EXECUTE still has command dispatch and can perform execution-time work; do not claim all binding/planning or runtime overhead is eliminated. Other feature/driver/engine differences remain. These are API latencies, not pure kernel CPU or disk durability comparisons.

This separate series improves the preparation contract, not production Lin speed. Do not derive a causal preparation speedup by comparing to the prior non-paired series: both engine timings shifted. Full named-peer goal still incomplete; small Rust SQLite and durable gaps, other native adapters, MSSQL/Kusto remain open. Earlier raw results and adapter snapshot preserved in ../2026-10-05-duckdb-native-api/.
