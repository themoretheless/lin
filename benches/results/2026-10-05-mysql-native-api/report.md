# MySQL native API bulk insertion

Source e5fed2a plus MySQL adapter; Lin production unchanged, pinned Lin worker from source 588136e, hashes recorded. Three independent processes per size, 24 fresh samples each, exact six-field values and multiplicity validated outside timing. Reports complete with no errors; require-wins exits zero. Nine offline benchmark-contract tests pass. Additional live escaped-input, uniqueness/rollback, owned cleanup and sentinel checks pass after timing. No build/test overlap.

| Rows | Lin median ms | MySQL median ms | MySQL / Lin | Lin process wins |
|---|---:|---:|---:|---:|
| 1000 | 1.506833 | 19.866187 | 13.184x | 3/3 |
| 10000 | 21.263687 | 175.011334 | 8.231x | 3/3 |

PyMySQL executemany uses its multi-row INSERT route. BEGIN, parameter escaping/SQL construction/transmission, server insertion and transaction commit included. Connection/schema/index/cursor/SQL-template setup, exact readback/drop excluded. Fresh owned InnoDB table per sample: id VARCHAR(255) primary, uri VARCHAR(512) unique, wing VARCHAR(64), title/body TEXT, ts BIGINT, wing/ts index. utf8mb4 is used; actual MySQL collation/string bounds differ from Lin, so identical arbitrary input semantics are not claimed. The fixture values fit all bounds. Identifier backticks escaped; cleanup only drops a table successfully created by that sample and rolls back before drop.

Existing dedicated mysql:8.4 container, data directory tmpfs, innodb_flush_log_at_trx_commit=1 and sync_binlog=1, recorded without weakening settings. Lin Db::empty is prepared Rust execution with default embedding/FTS and IPC excluded; MySQL includes driver, network, SQL parsing, WAL and commit. This is not equal feature costs, isolated engine CPU or physical-disk durability. It expands comparison coverage, not production Lin performance.

Live tests cover comma/quote/newline/Unicode/empty/backslash fields, duplicate URI rejection, rollback cleanup, no remaining owned fixture tables and sentinel preservation. Sentinel removed in finally. Container returned to original stopped state, verified in final-state.json. Dedicated local URL in test-live.py must be adapted before another environment is used. Official executemany behavior: https://pymysql.readthedocs.io/en/latest/modules/cursors.html . Remaining native adapters: MSSQL/Kusto/pandas; small Rust SQLite and durable SQLite gaps remain outstanding.
