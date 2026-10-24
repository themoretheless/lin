# Overlap vector WAL CPU with index/FTS maintenance — feasibility

2026-10-24 production baseline fa551af. No production change. Original sparse classification and serial/four-way sparse encoders copied from current persist.rs, real default rows/Arc embeddings generated outside timers. Actual public Store index/FTS/HNSW/row-map slab maintenance called on fresh Db::empty with wing/ts index. Serial control performs maintenance then vector encoding. Proposal launches scoped encode worker, performs maintenance on caller, joins before publishing rows. Thread creation/join and fresh vector-buffer allocation are timed. For <128 rows both paths are serial. Thread creation failure falls back to serial; no actual OS failure injected, and eligible successful runs assert worker used on every sample.

Vector Arc backing is shared with input rows, as in current WAL packing. Encoder reserves an estimated dense buffer as current typed writer does, includes existing sparse classification and parallel branch at10k; it does not pack scalar fields, construct full payload/frame, checksum, write or sync. Current10k helper may create inner workers, adding to thread/resource use. No claim about scheduler fairness, repeated-commit memory, or full-engine speed.

After timer: compare vector bytes to source-reference output; publish cloned rows (outside timer) then verify complete row values using explicit numeric take n, every row-ID position, FTS wal-hit count and indexed rag-filter count. FTS/filter validation checks counts, not full independent result-ID sets; this is not complete query-state equivalence proof. No maintenance implementation is replaced, only scheduling relative to read-only encoding changes. Independent production invariants and all error/rollback paths remain required before shipping.

Initial harness failure retained separately: take-all readback returned50 rows, so the first process aborted before any complete case report. initial-gate.rs, compressed initial-failure.log.gz, status/build/binary metadata retained. Corrected harness uses numeric take n and concise assertion output; invalid attempt excluded from all successful observations.

## First two-variant experiment

Two series ×six processes,24 rotating fresh samples/variant/case/process. All bytes/row assertions pass after harness correction. No runner builds/tests overlap timing; unrelated host activity uncontrolled. Fixed per-case ordering. This is1728 valid CPU work operations, not commit samples.

| Series | Fixture | Serial ms | Overlap ms | Median paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 0.636531 | 1.000750 | -53.509% | 0/6 |
| 1 | default_10k | 22.605291 | 8.101177 | +60.136% | 6/6 |
| 1 | small_127 | 0.074906 | 0.074885 | +0.031% | 3/6 |
| 2 | default_1k | 0.631083 | 0.419666 | +33.247% | 4/6 |
| 2 | default_10k | 3.984812 | 2.810437 | +30.678% | 6/6 |
| 2 | small_127 | 0.074021 | 0.074239 | -0.299% | 2/6 |

First1k -53.51% and second +33.25% disagree; first10k absolute times also vary substantially. No samples removed or causal host explanation assigned. This ambiguity motivated one matched-control calibration rather than accepting the favorable repeat.

## Three-variant calibrated gate

New pinned binary with same work and two identical serial controls A/B plus scoped-overlap C, rotated within each round; two independent series ×six processes,24 samples/variant/case/process (2592 valid operations). All eligible C samples assert worker used, both A/B serial; source and binary hashes checked. Load before processes retained. Scalar preparation/validation/output destruction outside timing. All three variants share the same execution function; scheduler and host load remain uncontrolled.

| Series | Fixture | Comparison | Serial A ms | Other ms | Median paired gain | Median absolute pair difference | Positive processes |
|---:|---|---|---:|---:|---:|---:|---:|
| 1 | default_1k | AA | 0.591229 | 0.587010 | -0.094% | 0.463% | 3/6 |
| 1 | default_1k | overlap | 0.591229 | 0.380958 | +34.319% | 34.319% | 6/6 |
| 1 | default_10k | AA | 3.942875 | 3.939802 | -0.189% | 0.310% | 1/6 |
| 1 | default_10k | overlap | 3.942875 | 2.761594 | +30.375% | 30.375% | 6/6 |
| 1 | small_127 | AA | 0.072698 | 0.072615 | -0.029% | 0.230% | 2/6 |
| 1 | small_127 | overlap | 0.072698 | 0.072875 | -0.173% | 0.173% | 1/6 |
| 2 | default_1k | AA | 0.610177 | 0.603990 | +1.019% | 1.019% | 6/6 |
| 2 | default_1k | overlap | 0.610177 | 0.442521 | +27.623% | 27.623% | 6/6 |
| 2 | default_10k | AA | 3.972729 | 3.961562 | +0.168% | 0.285% | 4/6 |
| 2 | default_10k | overlap | 3.972729 | 2.781750 | +30.616% | 30.616% | 6/6 |
| 2 | small_127 | AA | 0.073416 | 0.073427 | -0.042% | 0.198% | 3/6 |
| 2 | small_127 | overlap | 0.073416 | 0.073375 | +0.143% | 0.157% | 5/6 |

## Decision: advance to transaction prototype, not production acceptance

Calibrated target1k +34.32% /+27.62%,6/6 in each series, exceeds observed A/A absolute differences0.46% /1.02%.10k +30.38% /+30.62%,6/6 each;127-row serial control approximately flat. These are empirical component observations, not confidence intervals, a proof of stability under all loads or a cause assigned to the first negative series. Preserve that negative evidence when evaluating the next gate.

Next concrete integration constraints in integration-plan.md. Need owned staged payload/error propagation, exact legacy bytes, original failure ordering, serial fallback, buffer lifetime/RSS/cold/repeated-commit guards, full workspace, process-exit replay and calibrated full-engine tests. Full Db commit savings may be smaller or absent. No new public API, codec, disabled work or weakened durability is justified.

Production source unchanged verified by all saved core hashes. No workspace rerun for diagnostic-only source; library methods already have separate workspace evidence but that is not a transaction-prototype gate. No actual file writes/Full sync, power-cut, peer or RSS qualification here. Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas goal remains open; durable1kSQLite and absent live Microsoft endpoints persist.
