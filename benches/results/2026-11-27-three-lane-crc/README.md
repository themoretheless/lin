# Three-lane CRC32C in WAL/FULL INSERT

Candidate fedcfb9cb versus the fe2717127 engine and SQLite WAL/FULL. Both Lin workers use static TCMalloc on Linux x64 and the system allocator on ARM64. Each median subtracts timings within a pair. Negative values favor the candidate. The benchmark verifies recovered rows.

| CPU / host | Rows | Candidate − control (ms) | Wins / 24 | Candidate − SQLite (ms) | Wins / 24 |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 1000 | +0.029 | 9 | +0.223 | 1 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 10000 | +0.336 | 9 | -8.090 | 24 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 1000 | -0.047 | 18 | +0.348 | 6 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 10000 | -0.099 | 13 | -0.036 | 12 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 1000 | +0.007 | 12 | +0.441 | 1 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 10000 | -1.488 | 15 | +0.720 | 10 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 1000 | -0.126 | 20 | +0.355 | 1 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 10000 | -0.646 | 16 | +2.602 | 4 |
| Neoverse-N2 / wal-full-arm64 | 1000 | -0.044 | 16 | +0.503 | 0 |
| Neoverse-N2 / wal-full-arm64 | 10000 | +0.112 | 11 | -6.471 | 24 |

SQLite won the median 1k comparison on every host. [Raw paired measurements](https://github.com/themoretheless/lin/actions/runs/45999512399)
