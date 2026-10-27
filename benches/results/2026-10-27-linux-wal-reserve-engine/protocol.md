# Bounded Linux WAL reserve: paired full-engine protocol

Source baseline is the Rust source content of 08efc6e7f, including the transaction WAL generation/atomic frame correction. Candidate changes only src/persist.rs to the exact bounded Linux KEEP_SIZE prototype from 2026-10-27-linux-wal-keep-size-tx-fixed. Row remains the public BTreeMap alias. No allocator, synchronization, checksum format, journaling mode or payload semantics change.

Candidate functional tests and a missing-KEEP_SIZE runtime mutant must pass before measurement. Functional input retains rollback, follower, checkpoint and complete frame/reference CRC32C checks. test.py temporarily installs the candidate plus cfg-test oracle, restores all sources, and verifies restoration. Build compiles baseline and candidate separately with the same benchmark extension and restores the original source and benchmark files.

## Workloads and lifecycle

61 cases: all 24 previous Full-sync mutation cases (UPDATE, DELETE, interactive commit, rollback × 1k/10k × Lin/SQLite FULL DELETE/SQLite FULL WAL), plus all 37 native mutation, transaction, bulk/durable insertion and COUNT read controls. The benchmark source is the prior durable extension with the prior explicit COUNT preflight added outside timing. All queries and timed bodies are unchanged. Full-value clean-reopen oracles and affected-row assertions remain enabled; separate subprocess recovery tests qualify correctness, not timer observations.

Each round rotates case order and the three roles baseline/control/candidate. Baseline and control use the same uninstrumented binary. No warmup, eight observations, max iterations one, one operation per observation. Each observation uses fresh fixture, ordinary setup/teardown, and the existing DropPolicy::OutsideTiming contract. Seed/open/prepare costs remain outside operation timing exactly as before; fstat/fallocate triggered by the actual measured operation remain inside. This scope is prepared operations on seeded DBs, not cold-open latency or sustained throughput. Process RSS includes the entire lifecycle and must not be described as isolated operation memory.

12 rounds in two six-round series × 61 cases × 3 roles = 2196 isolated processes, 17568 observations. Every process captures resource data, binary hash, load before/after, exit status, raw result and stdout. No preload or audit libraries, no allocator tuning changes. Candidate/control uses the same runtime environment as baseline. Runner must be native Linux x86_64; local macOS smoke does not execute Linux reservation.

## Frozen acceptance

Six targets: Lin durable UPDATE, DELETE and interactive commit at 1k and 10k. Each target must have 6/6 positive process-pair gains and median gain greater than median absolute baseline/control A/A in both series. All 55 other cases are controls and require median gain >= -max(2%, A/A) in both series. These are the established source acceptance rules applied to actual WAL-write targets. Do not silently remove noisy controls or losses. Regressing targets or controls reject this candidate; source acceptance does not mean superiority over every external peer.

Report all within-round candidate-vs-peer ratios, including both SQLite journal modes and DuckDB reads. Candidate must not be promoted based on CPU microbench or a subset of targets. Strict power-loss qualification and full PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas comparisons remain separate requirements; full_objective_achieved stays false.

## Analysis integrity and runtime budget

summarize.py computes descriptive pairs and the frozen gate; verify.py independently reconstructs all medians, pairs, A/A, RSS, peer ratios, wins and gate decisions directly from raw files, source hashes and runtime metadata. test-protocol.py accepts a complete synthetic matrix, rejects missing rows/nonfinite values/altered RSS/altered summary, and forces gate failure for one losing target pair or a regressing control. Synthetic samples are validation fixtures, never benchmark results.

CI timeout is 120 minutes, accounting for full release tests, two builds, 122 correctness smoke runs and a larger timing matrix. Increasing time budget does not alter acceptance thresholds or observations. Upload artifacts even on cancellation/failure; incomplete runs cannot qualify a source change.
