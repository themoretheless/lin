# Linux transaction WAL generation qualification

[Run 40529581051](https://github.com/themoretheless/lin/actions/runs/40529581051) completed successfully at head 046226580717bec1f9a7af9e44d3ef5cc2134fd2, native Ubuntu x86_64, Rust 1.96.0. Full default release library/integration suite: **415 passed, 2 ignored, zero failed**. The frozen exec and regression test files match the saved candidate-source.json via Git-object SHA256 validation.

All eight dedicated transaction/WAL tests passed: single generation for in-memory commit; rollback and uncommitted Drop; one atomic frame for two updates; every strict frame prefix leaves a follower unchanged; checkpoint failure after publication retains committed state; actual subprocess exit before/after commit recovers none/all. Both GEN_PROBE records show base 1, next committed generation 2, one frame [2] and no follower gap; rollback leaves generation 1.

The successful source includes atomic commit/generation corrections, not the later aborted-transaction guard. It does not automatically qualify current exec changes or the Linux KEEP_SIZE performance prototype. This is process-exit/replay correctness evidence, not a power-loss proof or a benchmark win.

The same full-suite log contains the existing tests/tx_bench.rs informational timers. Its Normal-mode five-writes/transaction case prints Lin 161444 ns/tx and SQLite WAL synchronous=NORMAL 57176 ns/tx. This is a retained loss from one test process without paired controls, not accepted peer qualification; all other printed cases remain in the full log. The pending Full-sync one-row timing matrix does not cover this separate Normal-mode multi-write workload.

A second duplicate push run 40529586573 (same workflow ID, head SHA, branch and event) was intentionally cancelled; this successful run remains the authoritative completed qualification. Run metadata and the complete artifact log are preserved. Execute python3 benches/results/2026-10-27-tx-wal-generation-linux/verify.py for the independent scoped audit.
