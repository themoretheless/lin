# Row construction profile and hash formatting trial

The hash formatting candidate (7f4495aed) lost to control 2960abb85 at 1k on every measured host. In the separate baseline profile, generating the default vector was the largest of the three sampled row-building sections on every host at both sizes.

## Row construction on disk

Each value is the median over 16 inserts. Within each insert the section times are weighted by the number of sampled rows across worker threads. Section times are ns per sampled row. Build time is elapsed ms per batch. [Raw profile](https://github.com/themoretheless/lin/actions/runs/45994164168)

| CPU / replica | Rows | Schema | Vector | Cell fill | Batch build ms |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 1000 | 59.922 | 2552.125 | 594.500 | 1.213 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 10000 | 96.410 | 2753.775 | 604.512 | 9.284 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-2 | 1000 | 48.328 | 1089.953 | 292.234 | 0.536 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-2 | 10000 | 48.878 | 1236.048 | 317.532 | 4.103 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 1000 | 76.781 | 1304.438 | 329.453 | 0.654 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 10000 | 103.625 | 1366.265 | 374.438 | 4.742 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 1000 | 61.469 | 2569.672 | 589.047 | 1.210 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-4 | 10000 | 80.093 | 2746.003 | 599.090 | 9.013 |
| Neoverse-N2 / wal-full-arm64 | 1000 | 66.875 | 2716.922 | 618.719 | 1.254 |
| Neoverse-N2 / wal-full-arm64 | 10000 | 59.590 | 2046.735 | 594.054 | 7.787 |

## Full synchronous INSERT

SQLite and both Lin builds ran locally on each runner. Each delta is the median of 24 paired candidate-minus-reference times in ms, followed by candidate wins. [Raw comparison](https://github.com/themoretheless/lin/actions/runs/45993782033)

| CPU / replica | SQLite | Rows | Lin ms | Control ms | SQLite ms | vs control | vs SQLite |
|---|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 3.37.2 | 1000 | 2.495 | 2.485 | 2.530 | +0.035 (9/24) | -0.031 (15/24) |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-1 | 3.37.2 | 10000 | 29.764 | 29.565 | 29.957 | +0.230 (10/24) | -0.164 (14/24) |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 3.37.2 | 1000 | 1.484 | 1.484 | 1.887 | +0.012 (11/24) | -0.390 (23/24) |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-3 | 3.37.2 | 10000 | 18.761 | 16.868 | 18.912 | +1.470 (7/24) | -0.045 (12/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 1000 | 2.688 | 2.650 | 2.619 | +0.049 (11/24) | +0.089 (5/24) |
| Neoverse-N2 / wal-full-arm64 | 3.45.1 | 10000 | 25.024 | 24.932 | 26.998 | -0.031 (12/24) | -2.304 (23/24) |
