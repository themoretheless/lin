# MiMalloc prototype: rejected after independent repeat

Baseline production: 2f59f1a (production code identical to retained 4926d9c). Candidate selects mimalloc 0.1.52 only in the compare benchmark root and lib unit-test root; it does not change the CLI/library runtime allocator. Default mimalloc features, no C allocator override. Dependency: https://github.com/purpleprotocol/mimalloc_rust .

Each round uses six alternating baseline/candidate process pairs per mode, 24 fresh fixtures per case, one operation per sample, no warmup. Both rounds use the same pinned release binaries (SHA-256 recorded). Native and Full durable insert cases include Lin and SQLite at 1k and 10k. Figures are median paired percent latency improvement; positive is faster. Wins count process-median pairs, not individual samples.

| Case | First gain / wins | Repeat gain / wins |
|---|---:|---:|
| Lin native 1k | +10.205% / 5 of 6 | -9.621% / 2 of 6 |
| SQLite native 1k | +1.269% / 4 of 6 | -11.413% / 2 of 6 |
| Lin native 10k | +11.358% / 4 of 6 | +3.925% / 5 of 6 |
| SQLite native 10k | -1.819% / 2 of 6 | +14.148% / 6 of 6 |
| Lin durable 1k | +4.973% / 5 of 6 | -20.290% / 2 of 6 |
| SQLite durable 1k | +5.594% / 5 of 6 | -28.750% / 1 of 6 |
| Lin durable 10k | +1.741% / 4 of 6 | +9.747% / 3 of 6 |
| SQLite durable 10k | +6.043% / 4 of 6 | +7.710% / 5 of 6 |

The smaller workloads regress in the repeat and the control shifts materially. SQLite Rust wrappers and the harness also change allocator, so SQLite is not an unchanged control in this experiment. These measurements do not isolate allocator causality, prove CLI performance, or establish superiority over all named peers. Do not retain this prototype based on the first round.

Validation: corrected lib unit-test executable passes 45 tests under MiMalloc; corrected benchmark build and both paired runs complete. Initial failed logs document a prototype placement error (allocator inserted into bench prose and between an existing GPU cfg attribute and its module); placement was repaired from original snapshots before successful validation. No full workspace run was required for this rejected candidate. All four production files, including Cargo.lock, restored byte-for-byte from before snapshots. No dependency or allocator change ships.

Root and repeat medians.json contain process medians; run directories contain observations and fixture validation results. Candidate/before snapshots preserve the rejected implementation. MSSQL and Kusto endpoints remain unconfigured; small native and durable SQLite gaps remain unresolved.
