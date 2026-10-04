# Full named-peer coverage audit, 2026-10-05

Source 092bdae. No production code changed in this audit. The last goal turn produced evidence and rejected an index experiment; it was progress, not a blocked wait. This audit keeps the complete named-peer objective and does not declare it achieved on a subset.

| Peer | Six matched read APIs, 1k/10k/100k | Native bulk API 1k/10k | Outstanding evidence |
|---|---|---|---|
| SQLite | Aggregate wins in 2026-10-04 matrix | Python executemany wins in 2026-10-05 series | Rust-native 1k and Full durable 1k losses remain |
| DuckDB | Aggregate wins in 2026-10-04 matrix | Prepared DataFrame INSERT SELECT aggregate wins | API/feature-cost differences; no universal claim |
| PostgreSQL | Aggregate wins in 2026-10-04 matrix | COPY CSV API wins | Server/network/commit vs Db::empty; no equal disk durability proof |
| MySQL | Aggregate wins in 2026-10-04 matrix | Multi-row executemany API wins | Server/network/commit vs Db::empty; bounds/collation differences |
| MongoDB | Aggregate wins in 2026-10-04 matrix | Ordered insert_many wins in 2026-10-03 | Distinct write concern/atomicity; no disk durability equivalence |
| pandas | Aggregate wins in 2026-10-04 matrix | Not implemented | DataFrame creation is not silently counted as indexed transactional insert |
| MSSQL | Unconfigured, unmeasured | Not implemented | Dedicated test server access required |
| Kusto | Unconfigured, unmeasured | Not implemented | Dedicated test database access required |

Read sources: ../2026-10-04-current-peer-read-matrix/report.md (108/108 aggregate available comparisons, six APIs, three independent processes). Native insertion sources: ../2026-10-05-sqlite-native-api/report.md, ../2026-10-05-duckdb-prepared-native-api/report.md, ../2026-10-05-postgres-copy-native-api/report.md, ../2026-10-05-mysql-native-api/report.md, ../2026-10-03-mongo-native/report.md. Rust-native and durable unresolved cases are in ../2026-10-04-parallel-sparse-classifier/report.md, ../2026-10-04-current-peer-read-matrix/report.md and the later rejected experiments. The small local durable 10k lead does not close 1k losses. Historical results from different series are not combined as causal speedups or simultaneous eight-peer wins.

Fresh missing-peer gate verification: existing runner invoked with engines lin/mssql/kusto, point_get, 41 rows, two samples, two independent processes, require-wins. Exit 2, status incomplete. Both Lin observations validate, zero MSSQL/Kusto measurements are present; errors explicitly cite missing configuration. Thus absent peers are verified to remain incomplete, not counted as wins. This small run validates availability/gating only, not full performance. Configuration-presence.json records booleans only, never credential values. Request for dedicated endpoint configuration is pending; passwords/tokens should not be pasted into chat.

Remaining work toward the full objective: optimize the measured Rust SQLite and durable 1k gaps without weakening features/sync; resolve compatible write coverage with honest contracts; provision/receive dedicated MSSQL and Kusto access and actually run validated comparisons; verify all required matched cases with current sources and repetitions before completion. Endpoint absence blocks those two peers but does not justify marking the whole goal complete or blocked while useful local work remains.
