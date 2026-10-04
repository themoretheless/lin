# Rejected WAL vectored-write experiment

Baseline main 1edcdca. Prototype replaces four write_all calls per checksummed frame with write_vectored over stack IoSlice descriptors, advancing after short writes, retrying Interrupted and propagating WriteZero/other I/O errors. Payload, CRC, envelope and durable_sync ordering are unchanged. No extra payload copy or format change.

Eight focused WAL integrity tests passed, including the added injected short-write/interruption/WriteZero/BrokenPipe checks and existing frame/checksum/truncation/vector tests. Benchmark candidate built successfully. Full workspace was not rerun because the benchmark rejected the production change.

Six alternating baseline/candidate process pairs, 24 fresh samples per case, one operation/sample, no warmup, existing compare checked fixtures. Executables were pinned; Lin and SQLite both used their existing durable contract. These are end-to-end local API timings, not isolated syscall CPU timings. Each pair gain is computed from process medians and then the median paired percentage is reported; aggregate medians are listed separately. SQLite is an unchanged control.

| Case | Baseline median ms | Candidate median ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| compare/durable_insert_1k/lin | 3.462875 | 3.338084 | +1.021% | 3/6 |
| compare/durable_insert_1k/sqlite | 1.788750 | 1.798219 | -0.732% | 1/6 |
| compare/durable_insert_10k/lin | 22.598427 | 22.756584 | -0.058% | 3/6 |
| compare/durable_insert_10k/sqlite | 18.485709 | 18.384355 | +0.071% | 3/6 |

Neither fixture showed consistent gains (3/6 wins each). Reject the extra write loop complexity rather than infer a win from lower call count. This result does not prove syscalls are universally insignificant; the experiment only fails to demonstrate an improvement in the measured fixtures. The complete production source and new test were restored byte-for-byte to baseline. Candidate source, test/build logs, pinned executable hashes, raw process results and summaries remain here for review. The prior retained row-major optimization remains in main. Durable SQLite gaps and the full named-peer objective remain unresolved.
