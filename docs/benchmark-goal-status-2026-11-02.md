# Benchmark objective: current evidence, 2026-11-02

Objective remains open: outperform DuckDB, SQLite, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas under the declared benchmarks. No universal engine superiority or production adoption follows from this report. Main is unchanged. Current runtime with verified correctness is `60e64559ec0e5a21efd490d6da5a744e3ac8d0e2` (COW Arc keys plus explicit experimental extent WAL); public Row vnext migration is authorized, but this source has not passed the complete performance acceptance gate.

## Exact current six-peer matrix

[Run41413513001](https://github.com/themoretheless/lin/actions/runs/41413513001), frozen `8445e1e198e6020ec85c49d991650713dce8e251`, actual runtime60e64559e, Rust1.96.0 x86_64 Linux, default+experimental-extent-wal. Ubuntu22; SQLite3.37.2, DuckDB1.5.6, PostgreSQL16.15, MySQL8.4.13, MongoDB8.0.34, pandas3.0.6. Complete source/worker/derive/build closure, exact ELF, all15349 raw artifact members,18original processes and5184samples independently verified. Correctness, values, cleanup and source checks passed. Only the original all-process numerical win gate failed.

| Scope | Aggregate wins | Process wins | Remaining loss |
|---|---:|---:|---|
| Six read cases,1k/10k/100k, six peers |108/108|324/324|None in this matrix|
| Native insertion,1k/10k/100k, five peers |15/15|44/45|DuckDB100k, process1|

Read peers: SQLite, DuckDB, PostgreSQL, MySQL, MongoDB, pandas. Insertion peers: same set excluding pandas. Minimum aggregate read speedup is1.516x. DuckDB100k insertion aggregate: Lin219.226133ms versus DuckDB231.642704ms (peer/Lin1.0566381883). Its first process nevertheless loses: Lin228.915136ms versus DuckDB227.390245ms (peer/Lin0.9933386187). This loss stays binding; no unchanged rerun, trimming or threshold waiver.

Declared contract: prepared Rust Lin API versus Python APIs/server drivers and network, with full returned values/multiplicity validated. Default embeddings and FTS remain active. Native server tmpfs insertion differs from FULL WAL durable writes. Do not infer causal gains by comparing absolute times across different hosted runs.

[Complete peer tables](../benches/results/2026-11-02-cow-extent-public-peers-linux/report.md), [decision](../benches/results/2026-11-02-cow-extent-public-peers-linux/decision.md), [sole failed process](../benches/results/2026-11-02-cow-extent-public-peers-linux/failed-process-comparisons.json), raw archive and SHA manifests reside alongside them.

## Full source and SQLite durable qualification

[Run41401044023](https://github.com/themoretheless/lin/actions/runs/41401044023), frozen451fd7a25, actual runtime60e64559e versus BTreeMap baselinee3136c0a4. Complete67-case matrix:2412processes/19296observations and15244 raw artifact members audited. Exact Linux full default463/2 and extent-feature483/4 suites and five compiled negatives passed. All10 original target windows and48 Lin control windows passed; six unchanged SQLite control windows failed. External roles use one identical baseline-built ELF. No unique cause is proved, and these failures are not waived.

Absolute matched SQLite FULL WAL comparisons: **63/96 wins**, comprising1k UPDATE/DELETE/COMMIT36/36,10k UPDATE/DELETE/COMMIT27/36, rollback1k/10k0/24. Rollback1k Lin/SQLite medians1.1690/1.2527 across the two series;10k1.5309/1.6081. SQLite DELETE WAL and server tmpfs/native results are different contracts.

[Full decision and every failed window](../benches/results/2026-11-02-cow-extent-full-engine-linux/decision.md). The separate storage component's77/96 result belongs to an older runtime and is not pooled with63/96.

## Missing qualification and active experiments

- MSSQL: prior private encrypted qualification belongs to runtime625bc7180. [Current-source private qualification41423609660](https://github.com/themoretheless/lin/actions/runs/41423609660) completed;18original processes/1512samples independently audited from frozen c56375ce2, actual runtime60e64559e, root example default+extent. [Encrypted evidence](../benches/results/2026-11-02-cow-extent-mssql-private-linux/report.md) retained. No private outcomes, ratios, keys or plaintext are published here; functional completion is not a numerical win indicator.
- Kusto: real endpoint/database/token remain unavailable at the last checked inventory. No benchmark exists; no emulator substituted.
- FTS classification fold was rejected by its Linux component and reverted. It is absent from both current matrices.
- Constant768 token/trigram modulus: full local default463/2 and feature483/4 suites pass; local108-process native insertion discovery has no reliable gain. [Linux same-root-worker discovery41417236076](https://github.com/themoretheless/lin/actions/runs/41417236076) completed:100k-2.404%/+1.227%, only1/6and3/6positive. Rejected; original embedding source restored. All108processes/864samples retained.
- COW rollback ownership408f02f70 was rejected after all720local processes/5760observations were independently audited: only1/10target windows passed and3control windows failed. Full465/485tests and3compiled negatives passed, but avoided map replacement did not establish operational improvement. Runtime restored to60e64559e; [all results and decision](../benches/results/2026-11-02-rollback-row-ownership-bench/decision.md) retained. No unchanged Linux timing retry.
- Initial exact-root CPU profile41425364783 completed, but DWARF stacks inherited ephemeral CI credentials. Its public artifact13131138655 was deleted; raw originals are preserved only in the ignored owner-only private vault and were never committed. [Corrected profile41430732590](https://github.com/themoretheless/lin/actions/runs/41430732590) completed with an explicit four-variable environment; independent scans of all45rawfiles found zero credential patterns. All9527CPU samples and144worker samples audited with zero lost events. [Scoped report](../benches/results/2026-11-02-current-root-native-profile-linux/report.md) separates transaction ancestors from whole-process preparation. It is diagnostic, not benchmark acceptance.
- Extent growth and broader power-loss/directory qualification remain open. SIGKILL and checksum tests do not establish physical power-loss guarantees.

Next acceptance still requires the unchanged full source gate, all original public process comparisons, current-source private MSSQL qualification and a real Kusto run. Improvements, component results and green correctness checks do not redefine this scope.
