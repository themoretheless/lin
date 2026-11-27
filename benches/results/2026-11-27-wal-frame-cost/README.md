# Linux FULL commit frame profile

Profile build `0dd8d09f0`, based on `2960abb85`. Each block ran Lin and SQLite on disk and tmpfs in balanced order, with exact readback after reopening. Lin used TCMalloc on Linux x64 and the default allocator on ARM64. One FULL frame was measured per Lin insert. [Run and raw artifacts](https://github.com/themoretheless/lin/actions/runs/45988752104)

Times are milliseconds across 16 blocks. The disk columns include median Lin frame write, fdatasync, and total commit. The last two columns are median paired differences between Lin and SQLite on each medium. Positive differences favor SQLite.

| CPU / replica | Rows | Write | fdatasync | Lin total | Disk difference | tmpfs difference |
|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 1000 | 0.078 | 0.343 | 1.397 | -0.483 | -0.624 |
| AMD EPYC 9V45 96-Core Processor / wal-full-amd-1 | 10000 | 0.837 | 6.910 | 17.709 | -2.515 | -8.817 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 1000 | 0.203 | 0.410 | 2.548 | 0.005 | -0.229 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-2 | 10000 | 2.116 | 10.021 | 30.409 | 0.773 | -6.190 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 1000 | 0.207 | 0.420 | 2.606 | 0.047 | -0.190 |
| AMD EPYC 9V74 80-Core Processor / wal-full-amd-3 | 10000 | 2.114 | 7.592 | 28.051 | 0.927 | -6.284 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 1000 | 0.250 | 0.565 | 3.441 | -0.105 | -0.330 |
| AMD EPYC 7763 64-Core Processor / wal-full-amd-4 | 10000 | 2.584 | 2.129 | 27.149 | -8.484 | -9.457 |
| Neoverse-N2 / wal-full-arm64 | 1000 | 0.105 | 0.513 | 2.591 | 0.163 | 0.102 |
| Neoverse-N2 / wal-full-arm64 | 10000 | 0.742 | 9.363 | 25.208 | -2.187 | -7.693 |
