# Large WAL frames with optional LZ4

Candidate 430d8a911 uses FAST(16) for sparse FULL frames of at least 1 MiB. Both Lin workers use engine baseline fe2717127, static TCMalloc on Linux x64 and the system allocator on ARM64. The SQLite comparison uses WAL and synchronous=FULL on the same host. Exact readback after reopen is required.

Differences are medians of 24 within-block pairs per workload, in milliseconds. Negative favors the first operand. Wins count strictly faster measurements.

| CPU / replica | Rows | LZ4 − control | Wins | LZ4 − SQLite | Wins | Control − SQLite | Wins |
|---|---:|---:|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 1000 | +0.031 | 7/24 | +0.387 | 0/24 | +0.371 | 0/24 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 10000 | -4.987 | 23/24 | -2.490 | 22/24 | +2.408 | 5/24 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 1000 | +0.004 | 12/24 | +0.204 | 0/24 | +0.199 | 0/24 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 10000 | +2.016 | 1/24 | -5.209 | 24/24 | -7.228 | 24/24 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 1000 | -0.192 | 15/24 | +0.926 | 1/24 | +1.082 | 1/24 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 10000 | -6.217 | 21/24 | -2.901 | 20/24 | +3.584 | 0/24 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.049 | 9/24 | +0.334 | 1/24 | +0.280 | 2/24 |
| Neoverse-N2 / wal-full-arm64 | 10000 | +1.884 | 3/24 | -6.260 | 24/24 | -8.082 | 24/24 |

A local format check measured 336,981 bytes per INSERT frame for both workers at 1k. At 10k the control frame measured 3,558,353 bytes and the candidate frame measured 886,956 bytes. The current release cannot replay codec 7. [Linux run and artifacts](https://github.com/themoretheless/lin/actions/runs/45998091163)
