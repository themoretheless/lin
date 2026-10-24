# NEON sparse size count — CPU gate

2026-10-24 baseline da7e318. Explicit AArch64 NEON raw-u32 equality against zero, sixteen floats per chunk; four vectors reduced to a bounded 0..16 count, scalar tail. No fast-math or floating arithmetic; negative zero and NaN payloads stay nonzero. Every vector load is within a checked sixteen-element slice. Original sparse_bytes_serial extracted unchanged into reference.rs.

Two series of six processes, rotating baseline/candidate order, 24 samples each, warmed CPU component, no builds overlapping timing. Exact counts checked for 1025 lengths × four offsets using deterministic raw bits, plus None/Some(empty), zeros, negative zero, NaNs, infinities and tail. Real 1k/10k embedding fixtures and dense control. Source/binary hashes and build/runner/raw samples retained. This is not a DB-latency or peer victory claim.

| Series | Case | Baseline us | NEON us | Paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 89.097 | 40.145 | 54.89% | 6/6 |
| 1 | default_10k | 925.422 | 515.219 | 44.33% | 6/6 |
| 1 | dense | 89.502 | 40.811 | 54.63% | 6/6 |
| 1 | boundaries | 0.007 | 0.006 | 6.93% | 6/6 |
| 2 | default_1k | 89.581 | 40.486 | 54.85% | 6/6 |
| 2 | default_10k | 921.156 | 508.579 | 44.59% | 6/6 |
| 2 | dense | 89.485 | 40.515 | 54.83% | 6/6 |
| 2 | boundaries | 0.007 | 0.006 | 7.00% | 6/6 |

Candidate proceeds to engine qualification; production acceptance is not established by this report. The expected absolute 1k classifier saving is about 49 us, not a prediction of total commit improvement. Full peer goal remains open.

Follow-up [engine qualification](../2026-10-24-neon-count-engine/report.md) did not establish a calibrated DB gain; production candidate rejected.
