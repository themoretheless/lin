# Current production baseline — 2026-11-03

Runtime `0dab1a9a4f0f4f1e736d2e54f2e34a1e0f133d2d`. Fresh ordinary default `lin-peer-worker` package, unchanged root worker timers. macOS ARM64 discovery with pinned Python 3.14 bindings (DuckDB 2.0.0, pandas 3.0.6); unrelated host activity was not controlled. No cross-host latency pooling or eight-engine qualification.

At 1k/10k/100k, six read cases against SQLite, DuckDB and pandas: **54/54 aggregate wins and 162/162 process comparisons**, minimum aggregate peer/Lin 2.731x. Native memory insertion against SQLite/DuckDB: **6/6 aggregate wins but 17/18 process wins**. DuckDB 100k process3 remains a loss: Lin 97.235750ms versus DuckDB 86.907292ms. The native aggregate DuckDB100k margin is only 1.022935x. No trimming, unchanged numerical retry or universal dominance claim.

Driver-side full values and multiplicity were validated outside timers. Canonical native fields are id/uri/wing/title/ts/body, with default Lin embedding/FTS active. Prepared Rust APIs versus Python bindings are declared contracts; these are not FULL WAL comparisons. Raw timing/validation records, source digest manifest and all losing comparisons are retained; the independent audit reconstructs every parent/child process median and sample count.

Separately, [Linux CI41522176420](https://github.com/themoretheless/lin/actions/runs/41522176420) for the same runtime passed its narrower gate. Its single-process C++/Rust harness still loses agg_sum_by to DuckDB (peer/Lin 0.9879), bulk INSERT to SQLite, and numerous warm UPDATE/DELETE/rollback cases to SQLite. These losses are in audit.json; a green CI does not qualify all benchmarks. The contracts/hosts are separate and their absolute times are not compared causally.

Current production-source MSSQL qualification is retained separately as encrypted evidence. PostgreSQL/MySQL/MongoDB still need a fresh final-source matrix; historical COW results do not qualify this runtime. Kusto URL/database/token remain unavailable at the checked local/CI inventory. The original eight-engine goal remains open.
