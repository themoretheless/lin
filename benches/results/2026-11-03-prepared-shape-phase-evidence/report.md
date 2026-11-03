# Rejected prepared-shape source: phase diagnostic evidence

Run https://github.com/themoretheless/lin/actions/runs/41611885702 completed successfully at harness 0880db8acc81e65c82482563c82be64ce6ba0d0b, isolated runtime d8871eb36b119b9960989bf4a5feea9877168cc5. Independent audit reconstructed both patches byte-for-byte, verified both ordinary/instrumented source closures and x86_64 ELF binaries, all six-field result values, all phase events and all eighteen operation samples across six processes. Every archive member was reread and hashed. The earlier incompatible instrumentation attempt 41611764016 was cancelled; it is not a timing result.

| Rows | Ordinary operation median ms | Diagnostic operation median ms |
|---|---:|---:|
| 1k | 1.999519 | 2.027135 |
| 10k | 20.127512 | 20.212082 |
| 100k | 212.656466 | 220.827267 |

At 100k, each statement handles 10k rows. Diagnostic median per statement in microseconds: build 3166; validation 1793.5; embed 8363.5 (text 1682, vectors 5313, attach 1175.5); maintenance 9619 (index 3106.5, FTS 3717, row maps 2537.5). HNSW is inactive. Phase medians are not additive full-operation estimates. Diagnostic I/O and timers perturb execution; residual includes that overhead and uninstrumented work.

The historical baseline profile used a different runner: its vector phase was 14450 us while the current candidate is 5313 us, despite unchanged embed.rs. This cannot establish a causal vector improvement from the shape cache. Likewise ordinary operation medians from separate jobs are not a paired source qualification. Do not subtract these profiles into a speedup or overturn the frozen 216-process rejection. The large-case signal remains preserved but its mechanism is not established. Same-host diagnostic comparison would be needed to distinguish row-building effects from allocation/thread scheduling/compiler/runner differences.

The current branch runtime remains byte-identical to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. No source or peer qualification is claimed. FULL WAL and complete eight-peer superiority remain unfinished.
