# Linux paired mutation attempt: Cargo ANSI parsing failure

Run40390146614 on frozen12ad7a25 failed before measurements. Candidate tests and both contract sensitivity checks passed. Baseline release compilation succeeded after14m34s; Cargo emitted `Executable` followed by ANSI reset before the benchmark path. The human-output regex assumed a literal space and returned None. This is a harness error, not an engine compilation failure or a performance rejection. No run.json/performance samples were produced.

Full uploaded artifact and failed-step log retained with SHA256 manifest. Revised helper strips ANSI, requires exactly one matching benchmark executable and rejects missing/ambiguous paths. Build also requests --color never explicitly. Five helper regressions include the actual Linux log shape. Candidate source, workloads, sampling and acceptance thresholds stay unchanged.

Cache-on-failure enabled after verifying the upstream v2 action.yml declares that input; this preserves dependencies even if later qualification rejects a candidate. It is a build-cost change, not an engine speedup.
