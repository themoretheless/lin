# Release baseline versus SQLite WAL/FULL across Linux runs

Only the control worker and SQLite from each three-way experiment are compared. All control workers used the unmodified fe2717127 engine configuration (ThinLTO, static TCMalloc on x64, system allocator on ARM64). Every runner contributes one median of 24 paired differences per size. The summary gives those runner medians equal weight. Positive values mean Lin took longer. Runner assignments and underlying storage vary.

| CPU | Rows | Runner observations | Lin slower by runner median | Median difference (ms) | Range of runner medians (ms) |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor | 1000 | 7 | 7 | +0.210 | +0.163 to +0.444 |
| AMD EPYC 7763 64-Core Processor | 10000 | 7 | 0 | -7.511 | -8.011 to -6.885 |
| AMD EPYC 9V45 96-Core Processor | 1000 | 4 | 0 | -0.177 | -0.245 to -0.156 |
| AMD EPYC 9V45 96-Core Processor | 10000 | 4 | 3 | +0.384 | -1.829 to +1.195 |
| AMD EPYC 9V74 80-Core Processor | 1000 | 7 | 7 | +0.431 | +0.371 to +1.082 |
| AMD EPYC 9V74 80-Core Processor | 10000 | 7 | 7 | +2.524 | +0.440 to +4.642 |
| Neoverse-N2 | 1000 | 5 | 5 | +0.406 | +0.280 to +0.582 |
| Neoverse-N2 | 10000 | 5 | 0 | -2.087 | -8.082 to -1.574 |

## Source runs

- [Run 45994979312](https://github.com/themoretheless/lin/actions/runs/45994979312)
- [Run 45998091163](https://github.com/themoretheless/lin/actions/runs/45998091163)
- [Run 45999512399](https://github.com/themoretheless/lin/actions/runs/45999512399)
- [Run 45999726597](https://github.com/themoretheless/lin/actions/runs/45999726597)
- [Run 46002033662](https://github.com/themoretheless/lin/actions/runs/46002033662)
