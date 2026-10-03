# Inline FTS postings: rejected

Baseline: 7eaffc5. Candidate stores the first posting inline using SmallVec<[usize; 1]>; wire format and pending additions/deletions remain unchanged. Both sources and binary hashes are saved here.

Six independent alternating baseline/candidate pairs per workload, without overlapping compilation or tests. Insert workloads use 24 fresh fixtures per process, one operation per sample and no warmup; lexical reads use 32 samples, 50 ms warmup and up to 100 iterations. Exact fixture/read checks in the benchmark passed. Focused library FTS tests passed (12 tests); both benchmark binaries built successfully. Full workspace tests were not run for this rejected candidate.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| Native insert 1k | 2.34% | 5/6 |
| Native insert 10k | 1.38% | 5/6 |
| Durable insert 1k | -0.39% | 3/6 |
| Durable insert 10k | 1.41% | 5/6 |
| Lexical common | 4.84% | 4/6 |
| Lexical selective | 0.86% | 3/6 |
| Lexical miss | -3.09% | 2/6 |

Speedup is median(100 * (baseline_i - candidate_i) / baseline_i), not the ratio of separate aggregate medians. SQLite controls changed -0.28%, +1.11%, -1.27%, +0.21% respectively for the four insert workloads. The native 10k improvement is close to the control movement. Read timings show substantial process variability; the miss result does not establish a causal regression from inline storage.

Decision: reject this additional representation complexity given the small write improvements and inconclusive read results. src/fts.rs restored byte-for-byte to baseline. Raw run.json files, logs and summary medians remain for review. SQLite still wins native 1k and both durable sizes; Lin wins native 10k. This experiment does not complete the nine-engine performance goal.
