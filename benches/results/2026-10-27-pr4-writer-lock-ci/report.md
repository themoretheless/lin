# PR4 writer-lock CI benchmark evidence

GitHub run 40428499650 completed successfully for PR4 head `5906265c087299cf6b5cc099f284c361be96af04`. This is main205 plus the writer-lock correctness fix and Unix persistence CI; it is not the COW candidate. Original read/native and mutation artifacts are retained with raw run.json, HTML, progress and final status. Verify with `python3 benches/results/2026-10-27-pr4-writer-lock-ci/verify.py`.

Independent verification divides each raw wall observation by its operation count before taking its median: 568 observations across 71 read/native cases and 160 observations across 20 fresh mutation cases. Exactly eight samples per case, one process. Available same-run peers are SQLite/DuckDB; missing peers and absent durable workloads are not wins. This is observational CI evidence, not a controlled causal before/after or repeated-process performance qualification.

Read/native: 26/28 Lin wins; SQLite bulk insert remains slower: Lin/SQLite 1.121x (1k) and 1.329x (10k). Mutation: 0/10 Lin wins against SQLite. UPDATE/DELETE and transaction commit/rollback span Lin/SQLite 2.058x to 2.989x. Green CI does not establish the all-peer objective or that every workload wins. No speedup is attributed to the lock fix.

The COW-inplace A/A/B run 40428404395 is separate and still pending at collection time. Preserve all losers and raw samples in either outcome. Writer-lock PR Linux correctness CI is also still pending; successful bench is not a substitute for that check.
