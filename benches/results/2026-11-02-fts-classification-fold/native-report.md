# Three-state FTS fold: native insertion discovery

Same compiler and default+experimental-extent-wal features, same root peer_bench example and immutable worker source. Baseline binary comes from the archived earlier exact-feature functional preflight; all recorded build/source files match candidate except src/fts.rs. Base/control share that exact executable. Candidate changes only normalization plus its cfg(test) Unicode oracle. Existing worker operation timers, chunk quotas, transaction, FTS, embedding and exact data remain intact. Two balanced six-round series:108 processes /864 samples;3996000 full six-field row readbacks independently verified. No local build/test overlap. Unrelated host activity uncontrolled.

| Series | Rows | Gain % | AA % | Positive pairs |
|---|---:|---:|---:|---:|
| 1 | 1000 | -1.502 | 2.942 | 3/6 |
| 1 | 10000 | +7.963 | 6.995 | 6/6 |
| 1 | 100000 | +3.341 | 6.348 | 5/6 |
| 2 | 1000 | +4.029 | 3.441 | 5/6 |
| 2 | 10000 | +3.764 | 2.207 | 6/6 |
| 2 | 100000 | +6.387 | 1.972 | 6/6 |

Mixed result.10k improves in6/6 pairs in both series above the corresponding AA medians.100k first series remains below AA and only5/6 positive, second improves6/6;1k is not uniformly improved. No full-source gate, Linux result, external peer victory or production adoption follows. Preserve every window, including1k losses; do not pool with the standalone function prototype or older source candidates. The already-live451fd7a25 full matrix excludes this fold. Next requires exact-source Linux correctness and performance/peer qualification; previous COW/extent acceptance cannot be transferred.
