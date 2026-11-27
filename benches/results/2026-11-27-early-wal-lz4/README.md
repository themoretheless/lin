# LZ4 before CRC in the background encoder

Candidate 04178b7c8. Each cell is the median of 24 paired differences in milliseconds. Negative favors the candidate. The experiment compresses eligible sparse FULL frames starting at 1 MiB. Exact readback after recovery was required.

## Release main control

| CPU / runner | Rows | Candidate − control | Wins / 24 | Candidate − SQLite | Wins / 24 |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 1000 | +0.009 | 11 | +0.213 | 3 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 10000 | +2.424 | 1 | -4.805 | 24 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 1000 | -0.024 | 13 | +0.377 | 1 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 10000 | -5.225 | 23 | -3.422 | 23 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.010 | 12 | +0.423 | 0 |
| Neoverse-N2 / wal-full-arm64 | 10000 | -4.635 | 24 | -6.402 | 24 |

[Run and artifacts](https://github.com/themoretheless/lin/actions/runs/46005181028)

## Previous LZ4 writer control

| CPU / runner | Rows | Candidate − control | Wins / 24 | Candidate − SQLite | Wins / 24 |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 1000 | -0.023 | 13 | +0.269 | 2 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 10000 | -0.349 | 15 | -4.215 | 23 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 1000 | +0.009 | 11 | +0.228 | 0 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 10000 | -0.512 | 17 | -5.538 | 24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 1000 | -0.031 | 15 | -0.206 | 24 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-4 | 10000 | +0.401 | 9 | -4.301 | 22 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.012 | 10 | +0.403 | 2 |
| Neoverse-N2 / wal-full-arm64 | 10000 | +0.034 | 11 | -5.670 | 24 |

[Run and artifacts](https://github.com/themoretheless/lin/actions/runs/46005328224)
