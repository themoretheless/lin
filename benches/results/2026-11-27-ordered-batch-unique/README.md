# Ordered batch uniqueness in WAL/FULL INSERT

Candidate 2cc29cef3 versus the fe2717127 engine and SQLite WAL/FULL on the same runner. Each difference is the median of 24 paired elapsed times, in milliseconds. Negative favors Lin. All measured inserts required exact readback after reopen.

| CPU / runner | Rows | Candidate − main | Candidate − SQLite | Main − SQLite | Candidate wins over SQLite |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 1000 | +0.000 | -0.243 | -0.245 | 23/24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 10000 | -0.203 | -1.548 | -1.829 | 15/24 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 1000 | -0.065 | +0.281 | +0.322 | 3/24 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 10000 | +0.069 | -7.787 | -7.660 | 24/24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 1000 | -0.023 | -0.253 | -0.196 | 22/24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 10000 | -1.632 | -1.671 | +0.395 | 16/24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 1000 | -0.016 | -0.175 | -0.156 | 24/24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 10000 | +0.276 | +1.391 | +0.372 | 8/24 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.075 | +0.432 | +0.345 | 1/24 |
| Neoverse-N2 / wal-full-arm64 | 10000 | -0.529 | -2.590 | -2.087 | 21/24 |

The ARM64 1k loss persists. No EPYC 9V74 was allocated in this run. [Workflow and raw samples](https://github.com/themoretheless/lin/actions/runs/46002033662)
