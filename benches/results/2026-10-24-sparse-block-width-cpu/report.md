# Sparse encoder block widths: no production candidate

Source `42790b3aeb5c1380bc69534b0314f6f103ace9fe`. Production source remains unchanged. Trusted-fixture CPU-only experiment; no durable or peer win claimed.

The existing sparse encoder skips all-positive-zero eight-float blocks, then emits nonzero values in ascending index order. Compare identical encoders with widths 4, 8 (current), 16 and 32. No presence mask or trailing-zero traversal: this differs from the rejected October 3 mask experiment. Only chunk width and index multiplier differ. No format, sync, count-selection or production buffer reservation change.

Six independent release processes, 12 rotated variant-order timing rounds each; same preallocated output capacity for all variants, buffers cleared and reused. Fixture setup and exact byte checks outside timing. Default embeddings generated through current public Db API at 1k/10k, a dense forced-encoder control, and None/empty/+0/-0/NaN payload/Inf/partial-tail fixtures. Every output exactly matches the copied current serial encoder, including a nonempty output prefix. The source-derived reference is a byte-equivalence oracle, not an independent decoder or corruption qualification.

| Case | Width | Paired median gain | Faster processes |
|---|---:|---:|---:|
| boundaries | 4 | -13.523% | 0/6 |
| boundaries | 16 | +11.581% | 6/6 |
| boundaries | 32 | +14.811% | 6/6 |
| default_10k | 4 | -0.105% | 2/6 |
| default_10k | 16 | -0.045% | 3/6 |
| default_10k | 32 | -8.846% | 0/6 |
| default_1k | 4 | -1.369% | 0/6 |
| default_1k | 16 | +0.558% | 4/6 |
| default_1k | 32 | -17.866% | 0/6 |
| dense | 4 | +8.414% | 6/6 |
| dense | 16 | +6.145% | 6/6 |
| dense | 32 | -0.443% | 1/6 |

Reject width changes as a production hypothesis: width 4 and 32 regress the targeted 1k workload; width 16 only +0.56%, 4/6 processes and no meaningful 10k gain. Dense output here deliberately forces the sparse encoder and bypasses actual dense fallback, so its isolated gains cannot justify production policy. Tiny boundary timings are validation/control only. No end-to-end, RSS or additional workspace runs were warranted after this failed CPU gate. Current production block width remains eight.

All raw timing samples retained. Hashes identify core source, exact reference, diagnostic source and binary. Build completed, all six processes exited 0; temporary example removed. Source hashes verified unchanged. Full named-peer goal remains open, especially durable 1k and unavailable MSSQL/Kusto.
