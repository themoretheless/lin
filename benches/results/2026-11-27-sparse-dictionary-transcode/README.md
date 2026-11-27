# Dictionary WAL from sparse pairs

Candidate 550f82025 versus the fe2717127 engine and SQLite WAL/FULL. Each row summarizes 24 paired measurements after checking exact recovered values. Both Lin workers use static TCMalloc on x64 and the system allocator on ARM64. Negative differences favor the candidate.

| CPU / runner | Rows | Candidate − main (ms) | Wins / 24 | Candidate − SQLite (ms) | Wins / 24 |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 1000 | +0.232 | 1 | -0.027 | 18 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 10000 | -3.326 | 18 | -3.797 | 23 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 1000 | +0.222 | 0 | +0.582 | 0 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 10000 | -1.047 | 16 | +1.158 | 6 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 1000 | +0.296 | 7 | +0.145 | 7 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 10000 | -1.040 | 14 | -2.661 | 17 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 1000 | +0.268 | 4 | -0.038 | 16 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 10000 | -3.397 | 21 | -4.223 | 22 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.164 | 4 | +0.773 | 0 |
| Neoverse-N2 / wal-full-arm64 | 10000 | +2.408 | 3 | -4.494 | 24 |

The candidate increased median 1k latency on all five hosts. At 10k it improved on all four AMD hosts, but the 9V74 still trailed SQLite and ARM64 regressed versus main. [Run and artifacts](https://github.com/themoretheless/lin/actions/runs/46003623425)
