# SQLite WAL/FULL — row construction and encoding

The two trials used the same Lin control (`2960abb85`). Each runner executed both Lin builds and SQLite locally in balanced order and checked recovery after commit. Each difference is the median of 24 paired candidate-minus-reference times in milliseconds. Negative values favor the candidate. Parentheses give candidate wins out of 24.

## One pointer per row (53da10a50)

[Run and raw results](https://github.com/themoretheless/lin/actions/runs/45991884717)

| CPU / runner | SQLite | Rows | Lin ms | SQLite ms | Control ms | vs SQLite | vs control |
|---|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 3.37.2 | 1000 | 3.012 | 3.399 | 3.033 | -0.393 (23/24) | +0.009 (12/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 3.37.2 | 10000 | 25.530 | 35.166 | 25.974 | -9.553 (24/24) | -0.556 (14/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 3.37.2 | 1000 | 3.206 | 3.437 | 3.247 | -0.197 (19/24) | -0.019 (14/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-2 | 3.37.2 | 10000 | 26.111 | 35.415 | 26.292 | -9.201 (24/24) | -0.410 (13/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 3.37.2 | 1000 | 2.514 | 2.550 | 2.543 | -0.039 (17/24) | -0.030 (16/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 3.37.2 | 10000 | 29.912 | 29.417 | 29.904 | +0.759 (8/24) | -0.499 (13/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 3.37.2 | 1000 | 2.458 | 2.512 | 2.499 | -0.055 (20/24) | -0.052 (17/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 3.37.2 | 10000 | 28.341 | 26.762 | 28.897 | +0.366 (11/24) | -0.613 (13/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 1000 | 2.628 | 2.459 | 2.668 | +0.178 (2/24) | -0.044 (17/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 10000 | 25.614 | 27.463 | 25.308 | -1.931 (21/24) | +0.259 (11/24) |

## Two-way row construction for 512–1024 rows (b156f0e46)

[Run and raw results](https://github.com/themoretheless/lin/actions/runs/45992422851)

| CPU / runner | SQLite | Rows | Lin ms | SQLite ms | Control ms | vs SQLite | vs control |
|---|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 3.37.2 | 1000 | 3.717 | 3.432 | 3.212 | +0.292 (2/24) | +0.519 (0/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-1 | 3.37.2 | 10000 | 27.171 | 35.322 | 26.832 | -8.288 (24/24) | +0.518 (7/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 3.37.2 | 1000 | 3.168 | 2.610 | 2.687 | +0.562 (0/24) | +0.507 (1/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 3.37.2 | 10000 | 30.428 | 27.270 | 29.595 | -1.064 (13/24) | +2.070 (10/24) |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 3.37.2 | 1000 | 1.721 | 1.912 | 1.547 | -0.192 (24/24) | +0.174 (2/24) |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 3.37.2 | 10000 | 20.227 | 19.940 | 20.393 | +0.146 (12/24) | +0.167 (11/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 3.37.2 | 1000 | 3.782 | 3.357 | 3.061 | +0.251 (6/24) | +0.533 (0/24) |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 3.37.2 | 10000 | 25.641 | 35.112 | 25.687 | -9.549 (24/24) | -0.022 (12/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 1000 | 2.708 | 2.438 | 2.452 | +0.288 (0/24) | +0.271 (0/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 10000 | 16.485 | 24.551 | 16.551 | -8.064 (24/24) | -0.301 (14/24) |

The two-way candidate lost to the control in 117 of 120 pairs at 1k. The row-pointer candidate still lost to SQLite at 1k on ARM64 and at 10k on both EPYC 9V74 runners.
