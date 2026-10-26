# Persistent WAL encoder worker — CPU feasibility, 2026-10-26

Source baseline: `7cd747eafefaa226fbd3de5f552b043523f17b20`. No production source edits. This changes the mechanism relative to the rejected per-operation scoped worker: one persistent bounded-channel worker, `Arc<Vec<Row>>` ownership transfer, caller performs existing index/FTS/HNSW/map maintenance, worker releases its input Arc **before** replying. Caller must recover the Vec with `Arc::try_unwrap`; no deep row/text clone inside timing. Worker lifecycle is bounded by RAII join.

## Contract and controls

Source-derived sparse classifier/encoder helper block was checked byte-for-byte against current `src/persist.rs`. Default real HashingEmbedder fixture, original `[wing, ts]` scalar index and public Store maintenance APIs. Both serial controls and worker construct the vector-reference array inside timing, retaining the same existing sparse/parallel dispatch. `Arc` allocation/clone, reply channel creation, enqueue, synchronization, ownership recovery and output allocation are included. Input Vec cloning and fresh database/index setup are outside timing. The 127-row guard uses the serial path. Default vector fixtures are finite; this is not special-float qualification of a new codec.

Serial A, identical serial B and persistent-worker C rotate order within 24 rounds. Two series of six fresh processes, 2592 measured CPU compositions, plus 12 cold operations and 12 shutdown measurements. Original raw samples, process exit statuses, starting host load, binary hash and full source hashes retained. No runner build/test overlaps timing. Recorded starting load1:24.52–32.43; load is not CPU usage and does not identify a cause. There was no exclusive-host qualification.

Outside timing, every operation checks exact encoded bytes against serial reference, all public row values plus explicit embedding f32-bit comparison, every row-ID map entry, independent expected FTS/filter **ID sets** (not counts). Deterministic full/disconnected channel checks confirm input Arc release on rejected sends; they do not inject failures through the full engine. All 12 processes exit zero. Initial build attempt failed because `TrySendError::into_inner` is unavailable; corrected to dropping the rejected error/job, with the failed build log retained.

## Results

Positive gain means lower time. Gains are medians of within-process ratios, not ratios of aggregate medians. A/A absolute difference is the median absolute paired percentage difference.

| Case | Series | Worker gain | Positive processes | A/A absolute difference |
|---|---:|---:|---:|---:|
| 1k | 1 | +26.561% | 4/6 | 1.767% |
| 1k | 2 | +33.274% | 5/6 | 3.723% |
| 10k | 1 | +39.713% | 6/6 | 2.061% |
| 10k | 2 | +38.937% | 6/6 | 1.322% |
| 127 serial guard | 1 | +0.183% | 4/6 | 0.338% |
| 127 serial guard | 2 | +0.287% | 4/6 | 0.180% |

Do not discard 1k losses: process1 **−123.39%**, process2 **−59.30%**, process10 **−42.98%**. Candidate medians in those processes were 1.624/1.136/1.026ms versus serial0.727/0.713/0.718ms. This gate does not isolate scheduling, CPU placement or any other causal mechanism.

Cold 1k includes first worker creation, enqueue, maintenance, encoding, wait and ownership recovery: min/median/max **464/546/2440µs** across 12 fresh processes. It has no matched cold serial control, so no cold-start improvement is claimed. Worker shutdown+join:min/median/max **25.6/40.5/53.5µs**, outside the main operation timing.

## Decision and remaining gate

**Feasible CPU overlap at 10k; 1k remains unstable. Not accepted into production.** This experiment excludes parsing, row construction/embedding, scalar column packing, full WAL payload/frame, checksum, file write, sync and transaction handling. It establishes neither commit latency improvement nor peer superiority, and does not measure physical RSS, durable crash recovery, panic handling or multi-Db worker/resource limits. CPU timings cannot be combined with historical scoped-worker or full-engine numbers.

Next technically distinct experiment: integrate a bounded persistent encoder into the existing full WAL commit path in an isolated checkout, preserving error precedence, rollback, codec bytes and main-thread Full sync. First qualify a policy motivated by the stable10k result; include 1k and native as controls, without assuming the 1k CPU median justifies enabling it there. Recover ownership on queue-full/disconnection and worker panic; avoid one unbounded thread per Db. Require two conventional and interleaved A/A/B full-engine series, cold/repeated-commit/resource checks, exact replay and current SQLite control before acceptance. If full-engine gain fails, retain negative evidence and remove the source candidate.

All-eight-peer objective remains unmet: SQLite durable gap remains open; no new DuckDB/PostgreSQL/MySQL/MongoDB/pandas measurement, and MSSQL/Kusto have no measured server result here.
