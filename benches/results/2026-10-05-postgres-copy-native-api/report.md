# PostgreSQL native COPY CSV API insertion

Source b2a2dae plus PostgreSQL native adapter. Lin production unchanged; pinned worker from 588136e, binary/source hashes recorded. Three independent processes each size, 24 fresh samples/process, exact six-field values and multiplicities validated outside timers. Complete reports, no errors, require-wins exit zero. Existing nine offline contract tests pass. Additional live CSV/constraint/cleanup checks pass after timing. No tests/builds overlap measurements.

| Rows | Lin median ms | PostgreSQL median ms | PostgreSQL / Lin | Lin process wins |
|---|---:|---:|---:|---:|
| 1000 | 0.874188 | 5.176042 | 5.921x | 3/3 |
| 10000 | 11.427063 | 31.666375 | 2.771x | 3/3 |

Input CSV is fully quoted and encoded UTF-8 outside timing. Each PostgreSQL sample creates an owned unique table with primary id, unique uri and wing/ts index outside timing. Psycopg transaction BEGIN, COPY FROM STDIN CSV block, server acknowledgement and commit are timed. Cursor/query composition, connection creation, schema/index setup, exact readback/drop excluded. synchronous_commit remains on; server settings not weakened. PostgreSQL 16 runs in the existing dedicated postgres:16-alpine Docker container with data directory tmpfs. Lin Db::empty is in-memory prepared Rust execution with default embedding/FTS and IPC excluded. PostgreSQL includes network/driver/server/WAL commit costs. This is not equal durability, equal feature work, isolated CPU, physical-disk evidence or proof of every API's performance. Exact runtime versions/native contracts remain in run.json.

Live checks cover quoted commas, embedded quotes, newlines, Unicode, empty strings, duplicate URI rejection/transaction rollback, no remaining owned fixture tables, and an unrelated sentinel unchanged. Cleanup only drops tables successfully created by a sample; identifiers use psycopg.sql.Identifier. The sentinel is removed by its owner in finally. Owned PostgreSQL container returned to its original stopped state, verified in final-state.json. Test-live.py uses the dedicated local benchmark URL and should not be run on another database without adapting that URL.

Official protocol usage: https://www.psycopg.org/psycopg3/docs/basic/copy.html . Remaining native adapters: MySQL/MSSQL/Kusto/pandas; small native Rust SQLite and Full durable SQLite gaps remain open. This commit expands measurements, not Lin production performance.
