# Full-payload ownership: allocation diagnostic

Baseline: in-place Map Row + delta undo + bounded bitmap prefix without output recovery. Candidate: same source plus const-generic autocommit output recovery. This is not a production-main comparison.

Three fresh processes per variant, nine cells per process (wing UPDATE, title UPDATE, DELETE at 1k/10k/100k): 54 observations. Allocator counter covers prepared execution and full result-handle drop, including returned row contents; fixture setup and preparation are outside. All three repetitions produce identical counts per cell and variant. Every cell saves 11 allocation calls and 606 requested bytes. Counts are fixture/row-shape-specific and represent requested allocations, not RSS or peak/live memory. Raw counts include frees, retained in JSONL and summary.json. No timing or all-engine superiority follows from this diagnostic.

Readback checks affected count, generation, mutation fields/missing deleted row and indexed count. Separate full-row/reopen oracle and missing-output mutant are in ../2026-10-27-autocommit-pack-static/. Source and binary hashes retained in provenance.json; source restoration verified after builds and runs.

Pending independent Linux run: https://github.com/themoretheless/lin/actions/runs/40448137140 (source fc3ecc3cf520d9c7c91fd762add4951b7f9cfca1), comparing the combined candidate against production main with fresh isolated cases, SQLite controls, two six-round series and the unchanged acceptance gate. No result inferred while it is running.

External coverage check: current local environment has no LIN_BENCH_KUSTO_URL, LIN_BENCH_KUSTO_DATABASE or LIN_BENCH_KUSTO_TOKEN. This does not block other optimization/qualification work but leaves the Kusto part of the full eight-peer objective unproven.
