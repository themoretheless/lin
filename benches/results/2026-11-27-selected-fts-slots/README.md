# Selected FTS slots in WAL/FULL INSERT

Candidate 4c2bd66ad compared against the same engine baseline as fe2717127. Each size ran 24 balanced triples on each host, with exact readback after recovery. Times are paired median differences in milliseconds. Negative values favor the candidate.

| CPU / runner | Rows | Candidate − control | Wins / 24 | Candidate − SQLite | Wins / 24 |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 1000 | -0.036 | 13 | +0.431 | 1 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 10000 | +0.676 | 10 | +4.859 | 4 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 1000 | +0.016 | 11 | +0.457 | 1 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 10000 | +0.065 | 12 | -7.101 | 24 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 1000 | +0.029 | 10 | +0.179 | 0 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-3 | 10000 | +0.122 | 12 | -7.972 | 23 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 1000 | +0.015 | 8 | +0.133 | 3 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 10000 | +0.071 | 11 | -7.633 | 24 |
| Neoverse-N2 / wal-full-arm64 | 1000 | +0.001 | 12 | +0.486 | 0 |
| Neoverse-N2 / wal-full-arm64 | 10000 | +0.027 | 12 | -1.802 | 21 |

SQLite won 115 of 120 pairs at 1k. At 10k the candidate had higher median latency than the Lin control on every host. [Measurements](https://github.com/themoretheless/lin/actions/runs/45999726597)
