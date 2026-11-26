# SQLite WAL/FULL comparison with recovery verification

Each host ran 24 trials per size. Each trial measured Lin, a second invocation of the same Lin worker, and SQLite, with balanced ordering. Times are median elapsed milliseconds for the insert and commit. Lin replay and SQLite reopen checks ran outside the timer.

| CPU | Rows | Lin | Lin repeat control | SQLite | Lin wins / 24 |
|---|---:|---:|---:|---:|---:|
| EPYC 7763 | 1,000 | 2.809 | 2.818 | 3.413 | 23 |
| EPYC 7763 | 10,000 | 26.774 | 26.734 | 34.776 | 24 |
| EPYC 9V74 | 1,000 | 2.339 | 2.395 | 2.551 | 20 |
| EPYC 9V74 | 10,000 | 27.607 | 27.147 | 29.105 | 22 |
| ARM64 Neoverse N2 | 1,000 | 2.101 | 2.079 | 2.441 | 23 |
| ARM64 Neoverse N2 | 10,000 | 20.256 | 20.229 | 27.091 | 24 |

Lin won 136 of 144 paired trials. The smallest median margin was 0.212 ms at 1,000 rows on EPYC 9V74.

[CI artifacts and runner provenance](https://github.com/themoretheless/lin/actions/runs/45794293605)

## Uniform ID candidate

Engine candidate 154c986e4 ran the same balanced FULL-sync commit and recovery protocol. Three runners qualified. The other two x64 runners skipped measurement under the CPU filter. All measured jobs succeeded.

| CPU / runner | Rows | Lin ms | SQLite ms | Lin wins / 24 |
|---|---:|---:|---:|---:|
| EPYC 7763 / amd-1 | 1,000 | 3.103 | 3.602 | 24 |
| EPYC 7763 / amd-1 | 10,000 | 28.326 | 36.248 | 24 |
| EPYC 7763 / amd-3 | 1,000 | 2.868 | 3.457 | 23 |
| EPYC 7763 / amd-3 | 10,000 | 27.219 | 35.480 | 24 |
| Neoverse N2 | 1,000 | 2.079 | 2.440 | 24 |
| Neoverse N2 | 10,000 | 20.241 | 27.303 | 24 |

Lin won 143 of 144 pairs. This comparison measures the candidate against SQLite. Earlier revision timings came from different runners and cannot establish the candidate's incremental speedup.

[Paired samples and verified recovery run](https://github.com/themoretheless/lin/actions/runs/45817863320)

## Same-host uniform ID candidate versus previous Lin and SQLite

Balanced order, 24 pairs per host and size. Delta is the median paired candidate minus previous Lin in milliseconds. Negative values favor the candidate. All inserts were verified after recovery.

| Host | Rows | Candidate ms | Previous ms | SQLite ms | Delta ms | Wins vs previous | Wins vs SQLite |
|---|---:|---:|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-2) | 1000 | 1.411 | 1.390 | 1.973 | 0.023 | 10/24 | 23/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-2) | 10000 | 13.321 | 13.079 | 20.841 | 0.273 | 6/24 | 22/24 |
| AMD EPYC 9V74 80-Core Processor (wal-full-amd-3) | 1000 | 2.329 | 2.335 | 2.589 | -0.022 | 16/24 | 24/24 |
| AMD EPYC 9V74 80-Core Processor (wal-full-amd-3) | 10000 | 27.138 | 27.642 | 29.979 | -0.597 | 14/24 | 23/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-4) | 1000 | 1.325 | 1.263 | 1.924 | 0.012 | 11/24 | 22/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-4) | 10000 | 11.957 | 12.115 | 20.446 | -0.349 | 13/24 | 24/24 |
| Neoverse-N2 (wal-full-arm64) | 1000 | 2.165 | 2.108 | 2.412 | 0.052 | 8/24 | 18/24 |
| Neoverse-N2 (wal-full-arm64) | 10000 | 20.243 | 20.221 | 27.042 | 0.110 | 12/24 | 24/24 |

[Full-sync paired measurements](https://github.com/themoretheless/lin/actions/runs/45818768030)

### Decision on uniform ID presence

On ARM N2 the median paired candidate-minus-base difference at 1k was +0.062 ms when the candidate ran before the base and +0.041 ms in the reverse order (12 pairs per group). At 10k the two order groups had opposite signs (−0.256 and +0.327 ms). The distribution-free median intervals using the seventh and eighteenth ordered differences out of 24 span zero on ARM at both sizes (1k −0.045 to +0.136 ms, 10k −0.429 to +0.562 ms). One 9V45 host showed a positive 10k interval (+0.028 to +0.830 ms), while the other spanned zero. These are individual-host exploratory intervals. Given the ARM qualification condition and the remaining EPYC cold-INSERT deficit, the ID-presence shortcut was withdrawn. The separate field-order correctness fix and regression tests remain.

## Symmetric glibc THP comparison after withdrawing the ID shortcut

Four alternating mode blocks, six permutations of Lin, Lin control and SQLite per block and size. Each mode has 24 pairs. Every sample verifies recovered rows. Times are median milliseconds including commit.

| CPU | Runner | Rows | Default Lin / SQLite | Default wins | THP Lin / SQLite | THP wins |
|---|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor | wal-full-amd-1 | 1000 | 1.356 / 2.002 | 23/24 | 1.274 / 1.982 | 24/24 |
| AMD EPYC 9V45 96-Core Processor | wal-full-amd-1 | 10000 | 13.158 / 20.980 | 23/24 | 10.307 / 20.917 | 23/24 |
| AMD EPYC 9V74 80-Core Processor | wal-full-amd-3 | 1000 | 2.320 / 2.585 | 23/24 | 2.142 / 2.569 | 22/24 |
| AMD EPYC 9V74 80-Core Processor | wal-full-amd-3 | 10000 | 24.553 / 27.178 | 17/24 | 16.980 / 26.623 | 20/24 |
| AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 1000 | 2.306 / 2.556 | 22/24 | 2.063 / 2.547 | 24/24 |
| AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 10000 | 28.057 / 29.204 | 14/24 | 19.859 / 28.974 | 22/24 |
| Neoverse-N2 | wal-full-arm64 | 1000 | 2.080 / 2.404 | 23/24 | 1.841 / 2.412 | 24/24 |
| Neoverse-N2 | wal-full-arm64 | 10000 | 15.375 / 24.840 | 24/24 | 13.168 / 24.646 | 24/24 |

[Raw samples and runner provenance](https://github.com/themoretheless/lin/actions/runs/45820416806)

## Bulk buffer transfer candidate on Apple M4 Max

Revision 8950abf0a, macOS ARM64, SQLite 3.54.1. Four blocks cover all six process orders per size. Both engines reopen the durable database and validate every fixture row. The two Lin roles use the same binary.

| Rows | Lin median ms | SQLite median ms | Median paired Lin minus SQLite ms | Lin wins |
|---:|---:|---:|---:|---:|
| 1000 | 1.365 | 1.636 | -0.256 | 20/24 |
| 10000 | 6.915 | 15.434 | -8.649 | 24/24 |

On the same M4 Max, compare candidate 8950abf0a directly with Lin baseline 28d181d78. Each of 24 pairs per size alternated process order, used a fresh durable database for each binary, and checked every row after reopening. Median paired candidate minus baseline was −0.030 ms at 1k (14/24 wins) and +0.020 ms at 10k (11/24 wins). This comparison provides no consistent durable latency win.

## FTS prehash candidate on Apple M4 Max

Revision 2d408555d against SQLite 3.54.1, four balanced blocks with reopen and exact readback. The 1k batch falls below the 4096-row parallel FTS threshold.

| Rows | Lin median ms | SQLite median ms | Median paired Lin minus SQLite ms | Lin wins |
|---|---:|---:|---:|---:|
| 1000 | 1.454 | 1.420 | +0.057 | 7/24 |
| 10000 | 6.952 | 15.148 | -8.145 | 24/24 |

## Inline full-text keys on Linux

Engine 0add1938e, workflow revision 78465b33e. Each pair uses fresh databases and verifies recovered rows. The control uses the same Lin binary. Times are milliseconds.

| CPU and runner | Rows | Lin | SQLite | Median paired Lin minus SQLite | Lin wins |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-1) | 1000 | 1.251 | 1.923 | -0.675 | 24/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-1) | 10000 | 11.791 | 20.147 | -8.366 | 22/24 |
| AMD EPYC 7763 64-Core Processor (wal-full-amd-2) | 1000 | 2.998 | 3.384 | -0.407 | 23/24 |
| AMD EPYC 7763 64-Core Processor (wal-full-amd-2) | 10000 | 28.209 | 34.979 | -6.848 | 24/24 |
| AMD EPYC 7763 64-Core Processor (wal-full-amd-3) | 1000 | 2.885 | 3.486 | -0.604 | 24/24 |
| AMD EPYC 7763 64-Core Processor (wal-full-amd-3) | 10000 | 27.409 | 35.826 | -8.503 | 24/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-4) | 1000 | 1.751 | 2.033 | -0.325 | 15/24 |
| AMD EPYC 9V45 96-Core Processor (wal-full-amd-4) | 10000 | 13.492 | 20.783 | -7.656 | 22/24 |
| Neoverse-N2 (wal-full-arm64) | 1000 | 2.251 | 2.516 | -0.271 | 23/24 |
| Neoverse-N2 (wal-full-arm64) | 10000 | 16.294 | 25.029 | -8.774 | 24/24 |

Lin won 225/240 comparisons. [Linux artifacts and hardware provenance](https://github.com/themoretheless/lin/actions/runs/45848667483)

## Linux x64 TCMalloc candidate

Only the x64 Lin candidate loaded TCMalloc; SQLite and the same-binary Lin control kept their default allocation. ARM64 used default allocation throughout. Every sample used a fresh durable database and checked the six projected fields after reopening. Median paired differences in milliseconds are candidate minus peer. Each cell in parentheses gives candidate wins out of 24.

| Runner | 1k vs SQLite | 10k vs SQLite | 1k vs Lin control | 10k vs Lin control |
|---|---:|---:|---:|---:|
| EPYC 7763 / 1 | −0.798 (24) | −14.493 (24) | −0.230 (23) | −6.010 (24) |
| EPYC 9V74 / 2 | −0.520 (23) | −7.830 (24) | −0.267 (23) | −4.500 (23) |
| EPYC 9V45 / 3 | −0.595 (24) | −9.301 (24) | +0.011 (11) | −0.781 (17) |
| EPYC 9V74 / 4 | −0.504 (22) | −9.712 (22) | −0.279 (22) | −4.490 (22) |
| Neoverse-N2 | −0.371 (24) | −7.082 (24) | −0.056 (16) | +0.047 (10) |

[Allocator provenance, samples and recovery checks](https://github.com/themoretheless/lin/actions/runs/45865979433)
