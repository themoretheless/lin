# Combined COW + CAS full-source decision

Run40607797376 at frozen36666994a completed all2412fresh processes/19296observations. Independent frozen-source/functional/raw audit passed. All10target windows passed6/6aboveA/A: native prepared UPDATE gains13.9409–20.2672%,scalar Full52.0360–52.9989%,Normal94.6509–94.6796% against corrected BTreeMap6812.

The full source gate failed. Seven control windows failed: Lin bulk INSERT10k in bothseries(-5.6826%/-7.7117%,A/A2.1249%/1.2026%,0/6positiveboth),Lin filter_eq series1(-5.2428%,A/A0.6430%,0/6),Lin durable UPDATE1k series2(-2.8773%,A/A1.5293%),unchanged SQLite WAL commit10k/rollback10k and SQLite tx_commit100k inseries1. The unchanged-peer failures do not explain away the Lin regressions; no unique causal explanation is established.

Candidate peer wins in this contract: SQLite DELETE-journal96/96,SQLite WAL3/96,native/read SQLite49/228,DuckDB36/36. These are456comparisons within this frozen engine harness, not all-eight-peer qualification. Fresh Kusto evidence is absent and MSSQL comparison remains private. The objective remains incomplete.

No production adoption or unchanged rerun. COW development remains authorized for the next API version; authorization does not waive this gate. The later SoA candidate c60cb1f10 is a separate experiment and cannot inherit full-source acceptance from this run. Its unchanged-text optimization also does not explain or automatically fix fresh bulk INSERT/filter regressions.

All observations,failed pairs,resource records and exact input assets remain in raw-artifact.tar.gz. raw-manifest.json verifies every archive member and the archive hash. The unpacked artifact is retained locally. The next full-source candidate must address bulk/filter regressions and durable write cost without changing peer fixtures,durability or acceptance thresholds.
