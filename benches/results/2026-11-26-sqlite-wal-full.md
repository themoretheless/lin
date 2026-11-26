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
