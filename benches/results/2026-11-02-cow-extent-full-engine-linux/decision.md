# Combined COW Arc and extent WAL: full gate not accepted

Exact full run41401044023 at451fd7a25. Independent audit archived15244 members,2412 processes/19296 observations and1368 invariant external assignments; exact ELFs and frozen source/test/correctness blobs verified. All10 original target windows passed, as did48 Lin control windows. Six immutable SQLite windows failed the unchanged full gate; no unique cause is established and no gate waiver or unchanged retry is allowed. This is not accepted production source.

SQLite FULL WAL matched comparisons:63/96 wins in this full run. This differs from the separate77/96 storage component; no cross-run causal inference is made.

| Operation | Rows | S1 Lin/SQLite | S1 wins | S2 Lin/SQLite | S2 wins |
|---|---|---:|---:|---:|---:|
| update | 1k | 0.7831 | 6/6 | 0.7832 | 6/6 |
| delete | 1k | 0.7956 | 6/6 | 0.8012 | 6/6 |
| commit | 1k | 0.7307 | 6/6 | 0.7793 | 6/6 |
| rollback | 1k | 1.1690 | 0/6 | 1.2527 | 0/6 |
| update | 10k | 0.9763 | 4/6 | 0.9004 | 6/6 |
| delete | 10k | 0.9387 | 5/6 | 0.9588 | 4/6 |
| commit | 10k | 0.9623 | 4/6 | 0.9588 | 4/6 |
| rollback | 10k | 1.5309 | 0/6 | 1.6081 | 0/6 |

Failed windows (positive gain means faster):

| Case | Series | Gain % | AA % | Positive pairs |
|---|---|---:|---:|---:|
| compare/durable_commit_1row_1k/sqlite_wal | series1 | -6.431 | 4.990 | 1/6 |
| compare/durable_update_1row_10k/sqlite_wal | series2 | -6.479 | 3.423 | 2/6 |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | -5.875 | 5.728 | 0/6 |
| compare/update_1row_1k/sqlite | series1 | -2.803 | 1.711 | 1/6 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -2.111 | 1.641 | 2/6 |
| compare/tx_commit_1row_100k/sqlite | series1 | -19.729 | 19.528 | 1/6 |

Actual six-public-peer discovery launched separately as41413513001 from restored source60e64559e. Even a successful public matrix cannot erase this failed source gate or qualify MSSQL/Kusto. FTS classification fold9e15eca1a was separately rejected by Linux component evidence and reverted; it is not present in either full or public run. Main remains unchanged; the complete eight-peer objective remains false.
