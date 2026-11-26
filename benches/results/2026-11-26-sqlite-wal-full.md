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
