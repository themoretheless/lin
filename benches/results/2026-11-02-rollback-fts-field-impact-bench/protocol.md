# Rollback FTS journal field-impact comparison

Baseline: exact corrected COW 625bc7180. Candidate: exact 65bc82590. Same Cargo/dependency inputs and unchanged frozen compare.rs harness from the balanced complete COW comparison (SHA256 59b8acb6b9157d6b5fde56332fb35b0f2e0ea4728eb7f6a6b8c0c53c14b58508). The only runtime change uses RowDelta field names to skip FTS restoration for unrelated fields. Original row restoration, secondary indexes, maps, output ownership, quotas and durability remain intact.

Five targets: warm rollback at 1k/10k/100k, FULL rollback at 1k/10k. Fifteen controls include warm/FULL UPDATE, COMMIT, DELETE, bulk INSERT, scalar FULL/NORMAL, selective FTS, SQLite rollback, SQLite DELETE and DuckDB GROUP BY. All 20 cases and all losing outcomes retained. This component comparison cannot qualify whole-engine adoption or superiority over eight peers.

Twelve rounds in two six-pair series. Three roles: baseline, identical-binary baseline control, candidate. All six role permutations per case per six rounds; every position twice, each ordered adjacency twice, each pairwise precedence three times. Case rotation uses separate original indices. Execute the prevalidated schedule: 720 fresh processes, eight observations each, 5760 total. Forty fresh functional smokes precede timing.

Use each process's median wall observation. Per series target requires all six gains positive and median gain above median absolute A/A; control requires median gain >= -max(2%, median absolute A/A). No changed thresholds, deleted controls or noise waiver. Same native API contracts as the existing harness; no durability equality claim beyond its declared workloads.

Fresh candidate release suite must pass 460 tests / 2 ignored; both compiled FTS-impact mutants must fail named runtime assertions; restored focused tests and source-bound functional audit must pass. Build baseline then candidate serially in this job's private Cargo target, copy each executable before the next build, and restore all original source bytes. Never share the target with another worktree or concurrent build.

Independent verifier checks source refs and hashes, full functional proof, live binary identities, all 40 smokes, exact commands, raw order properties, unique IDs, all 5760 values, pair/series arithmetic and the complete gate. Synthetic tests reject the old order and corrupted inputs; losing targets and controls remain valid data but fail acceptance. Linux binaries are retained in a separate CI artifact. No source adoption until required qualification passes.

Local preflight attempt1 used the wrong top-level harness snapshot and stopped at a missing-case smoke (0 measurement processes). Its source/input manifest and raw outputs are preserved separately. Corrected preflight uses the existing full-COW harness unchanged.
