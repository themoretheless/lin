# PR5 take-all CI benchmark evidence

GitHub run40429347414 completed successfully for PR5 head c914eea9fe428dd83df3f92b836fc264e4903fa6, based on main205. This source includes explicit-unlimited-take correctness and query CI; it does not include COW and predates the writer-lock merge. Original read/native and mutation artifacts are retained. verify.py independently divides raw wall samples by operation count and checks exactly eight samples per case: 568 observations across71 read/native cases and160 across20 fresh mutation cases.

Available SQLite/DuckDB read/native comparisons:26wins,2losses. Lin/SQLite bulk insert is1.237x at1k and1.322x at10k. Mutation comparisons:0/10 Lin wins; Lin/SQLite1.748x–2.570x. All ratios and losers remain in verified-summary.json. Missing peers, excluded durable scenarios and comparisons without a peer are not wins.

This is one-process observational CI evidence, not a causal comparison to PR4 or a repeated-process speed qualification. No speedup is attributed to the take-all fix. Green configured CI does not establish all-case or all-eight-peer superiority. The separate frozen COW case-isolated run40431125318 is pending at collection time.
