# Linux release LTO comparison

Engine baseline: `fe2717127`. Lin uses static TCMalloc on Linux x64 and the system allocator on ARM64. Each cell reports the median paired difference in milliseconds. Negative values favor the first implementation in the column heading. Wins count strictly faster measurements.

## duckdb ([run 45994978902](https://github.com/themoretheless/lin/actions/runs/45994978902))

| CPU / replica | Rows | Operation | Thin − peer (ms) | Thin wins | Fat − Thin (ms) | Fat − peer (ms) |
|---|---:|---|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / amd-1 | 10000 | first | -2.876 | 25/25 | +0.104 | -2.876 |
| AMD EPYC 9V74 80-Core Processor / amd-1 | 10000 | repeat | -10.110 | 25/25 | +0.021 | -10.068 |
| AMD EPYC 9V74 80-Core Processor / amd-1 | 100000 | first | +13.048 | 0/25 | +1.106 | +14.121 |
| AMD EPYC 9V74 80-Core Processor / amd-1 | 100000 | repeat | -58.090 | 25/25 | +1.163 | -56.976 |
| AMD EPYC 7763 64-Core Processor / amd-2 | 10000 | first | -3.280 | 25/25 | +0.201 | -2.921 |
| AMD EPYC 7763 64-Core Processor / amd-2 | 10000 | repeat | -12.300 | 25/25 | +0.109 | -12.012 |
| AMD EPYC 7763 64-Core Processor / amd-2 | 100000 | first | +11.834 | 0/25 | +0.332 | +12.569 |
| AMD EPYC 7763 64-Core Processor / amd-2 | 100000 | repeat | -80.871 | 25/25 | +1.491 | -79.842 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 10000 | first | -2.890 | 25/25 | -0.149 | -2.826 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 10000 | repeat | -10.046 | 25/25 | +0.065 | -10.114 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 100000 | first | +14.007 | 0/25 | -0.397 | +13.130 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 100000 | repeat | -61.274 | 25/25 | +0.240 | -61.129 |
| Neoverse-N2 / arm64 | 10000 | first | -11.072 | 25/25 | -0.053 | -11.039 |
| Neoverse-N2 / arm64 | 10000 | repeat | -17.740 | 25/25 | -0.052 | -17.789 |
| Neoverse-N2 / arm64 | 100000 | first | -89.599 | 25/25 | -0.138 | -89.915 |
| Neoverse-N2 / arm64 | 100000 | repeat | -151.793 | 25/25 | +0.462 | -151.092 |

## sqlite ([run 45994979312](https://github.com/themoretheless/lin/actions/runs/45994979312))

| CPU / replica | Rows | Operation | Thin − peer (ms) | Thin wins | Fat − Thin (ms) | Fat − peer (ms) |
|---|---:|---|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / 1 | 1000 | WAL/FULL | +0.292 | 1/24 | +0.068 | +0.300 |
| AMD EPYC 7763 64-Core Processor / 1 | 10000 | WAL/FULL | -6.885 | 24/24 | +0.374 | -6.757 |
| AMD EPYC 9V74 80-Core Processor / 2 | 1000 | WAL/FULL | +0.451 | 3/24 | -0.017 | +0.421 |
| AMD EPYC 9V74 80-Core Processor / 2 | 10000 | WAL/FULL | +0.440 | 12/24 | +0.994 | +2.273 |
| AMD EPYC 9V45 96-Core Processor / 4 | 1000 | WAL/FULL | -0.158 | 15/24 | +0.042 | -0.013 |
| AMD EPYC 9V45 96-Core Processor / 4 | 10000 | WAL/FULL | +1.195 | 4/24 | -0.230 | +0.669 |
| Neoverse-N2 / arm64 | 1000 | WAL/FULL | +0.406 | 0/24 | -0.071 | +0.416 |
| Neoverse-N2 / arm64 | 10000 | WAL/FULL | -1.574 | 19/24 | +0.154 | -1.069 |

DuckDB 2.0.0 runs in memory. First and repeat refer to position within the process, with a fresh database per insertion. Each repeat observation is the median of four timed insertions. SQLite used WAL with synchronous=FULL and validated readback. The x64 hosts used SQLite 3.37.2 and ARM64 used 3.45.1. Hosts filtered out by CPU selection contribute no measurements.

The first 100k insertion on EPYC lost to DuckDB in all 75 pairs for each build. Full LTO did not close that gap.
