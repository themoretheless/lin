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

## In-memory INSERT versus DuckDB 2.0.0

Median paired differences in milliseconds over 25 groups. Repeat is the median of the next four fresh-database insertions in the same process. Both engines ran on the same host.

| CPU / runner | Rows | INSERT | Candidate − control | Candidate − DuckDB | Wins versus DuckDB / 25 |
|---|---:|---|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-1 | 10000 | first | -0.087 | -2.945 | 25 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-1 | 10000 | repeat | -0.070 | -10.402 | 25 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-1 | 100000 | first | -1.191 | +9.417 | 0 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-1 | 100000 | repeat | +0.021 | -63.475 | 25 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-2 | 10000 | first | +0.107 | -3.806 | 24 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-2 | 10000 | repeat | +0.069 | -13.074 | 25 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-2 | 100000 | first | +1.677 | +10.958 | 0 |
| AMD EPYC 9V74 80-Core Processor / paired-duckdb-insert-amd-2 | 100000 | repeat | +1.482 | -86.023 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-3 | 10000 | first | +0.124 | -7.045 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-3 | 10000 | repeat | +0.158 | -9.537 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-3 | 100000 | first | -1.200 | -42.622 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-3 | 100000 | repeat | -0.642 | -69.056 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-4 | 10000 | first | -0.052 | -7.151 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-4 | 10000 | repeat | +0.043 | -9.631 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-4 | 100000 | first | +0.245 | -44.495 | 25 |
| AMD EPYC 9V45 96-Core Processor / paired-duckdb-insert-amd-4 | 100000 | repeat | -0.081 | -69.587 | 25 |
| Neoverse-N2 / paired-duckdb-insert-arm64 | 10000 | first | +0.036 | -11.208 | 25 |
| Neoverse-N2 / paired-duckdb-insert-arm64 | 10000 | repeat | +0.008 | -17.888 | 25 |
| Neoverse-N2 / paired-duckdb-insert-arm64 | 100000 | first | +0.308 | -91.127 | 25 |
| Neoverse-N2 / paired-duckdb-insert-arm64 | 100000 | repeat | -1.125 | -153.200 | 25 |

DuckDB won every first 100k pair on both EPYC 9V74 hosts. Candidate versus control changed sign between those hosts. [Paired run and artifacts](https://github.com/themoretheless/lin/actions/runs/45999726622)
