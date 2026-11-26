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
