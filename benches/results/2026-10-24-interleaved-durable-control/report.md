# Interleaved durable A/A protocol qualification

Production source `919d94834eace149bad75ceac4b0072f2cb52bac`; no engine edits. The initial WAL audit found one normal commit/frame/Full flush for a fresh prepared bulk insertion (schema/setup and Db drop/checkpoint are outside operation timing). No redundant flush removal was justified. Snapshot threshold is 32 writes, above this fresh fixture. This source audit is not physical-power durability proof.

## New diagnostic protocol

Two persistent release workers are byte-identical executable copies. Each request creates a fresh owned temporary directory, opens default Full Db, creates wing/ts index and prepares the standard-shaped docs insert outside timing. Only prepared.run, including WAL and Full sync, is timed; returned Handle destruction is outside timing. Nine projected fields, including hash and every embedding f32 bit, are compared to a same-source in-memory reference after each insertion. Reference construction, readback, Db drop/checkpoint, fixture removal and JSON IPC are outside timing. No embeddings/FTS/indexes/quotas are disabled. Fixed timestamps keep exact comparisons deterministic. This is a separately qualified fixture/measurement protocol, not a new SQL peer comparison or recovery test. The memory oracle is not independent of the engine.

Within each process pair, requests alternate sample-by-sample, reversing baseline/candidate order on consecutive rounds. Both workers are idle while the other runs. One checked preflight request per worker is excluded, then 24 measured samples per worker. Six fresh worker pairs per size (1k/10k), followed by an independent repeat with the exact same binaries/scripts. 48 worker processes exited zero; all 1152 measured requests and 48 preflights passed value/bit checks. Owned fixture cleanup succeeded; no matching directories remained. No build/test overlap; unrelated host activity uncontrolled and load averages retained. The first diagnostic compile referenced an unavailable tempfile dependency; replaced with create-new owned directories, no dependency added, original build log retained.

| Series | Rows | Apparent paired gain | Median absolute pair difference | Max absolute pair difference |
|---|---:|---:|---:|---:|
| first | 1000 | -0.109% | 2.062% | 4.712% |
| first | 10000 | -0.141% | 0.515% | 4.476% |
| repeat | 1000 | -0.994% | 3.102% | 8.249% |
| repeat | 10000 | -0.129% | 0.737% | 1.285% |

## Interpretation

The earlier process-block A/A showed durable 1k median absolute pair difference 12.54%. This campaign observed 2.06% then 3.10% for the interleaved protocol, with remaining individual differences as high as 8.25%. Different times, fixture lifecycle and protocol prevent assigning the reduction solely to interleaving or treating it as a universal noise threshold. These are empirical observations, not confidence intervals. Near-zero signed medians alone cannot prove stability.

Use this worker/protocol as an additional affected-Lin control for a future candidate, retaining independent repetition and same-campaign A/A calibration; it does not replace Rust native/10k guards or SQLite/other-peer measurements. Small differences comparable to no-change variation remain inconclusive. Do not revive rejected candidates based only on this control or rerun until a desired win.

Production remains unchanged. Durable 1k SQLite gap, MSSQL/Kusto live qualification and full objective remain open. This delivery improves attribution/measurement capability, not engine throughput.
