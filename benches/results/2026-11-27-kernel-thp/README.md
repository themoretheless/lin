# Kernel transparent huge pages and prepared INSERT

Lin engine fe2717127 and DuckDB 2.0.0 ran in fresh processes on the same Linux runner. The harness alternated the kernel policy between madvise and always, verified it before each role, and validated the inserted rows. Each workload has 24 balanced comparison groups. First refers to the first transaction in a process. Repeat takes the median of the following four transactions. Each transaction uses a fresh in-memory database.

Times are median paired differences in milliseconds. Negative values favor Lin. The final column counts Lin wins with the always policy.

| CPU / replica | Rows | INSERT | Lin − DuckDB, madvise | Lin − DuckDB, always | Lin always − Lin madvise | Wins, always |
|---|---:|---|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / amd-3 | 10000 | first | -3.023 | -1.165 | +0.151 | 22/24 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 10000 | repeat | -10.317 | -10.179 | -0.109 | 24/24 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 100000 | first | +12.007 | +5.222 | -13.629 | 0/24 |
| AMD EPYC 9V74 80-Core Processor / amd-3 | 100000 | repeat | -61.295 | -57.385 | -1.365 | 24/24 |
| AMD EPYC 9V74 80-Core Processor / amd-4 | 10000 | first | -2.960 | -1.214 | +0.264 | 24/24 |
| AMD EPYC 9V74 80-Core Processor / amd-4 | 10000 | repeat | -10.269 | -9.927 | +0.072 | 24/24 |
| AMD EPYC 9V74 80-Core Processor / amd-4 | 100000 | first | +12.826 | +6.116 | -13.418 | 0/24 |
| AMD EPYC 9V74 80-Core Processor / amd-4 | 100000 | repeat | -60.956 | -55.317 | -1.064 | 24/24 |

Only runners with complete uploaded timing samples appear in the table. [Run and raw samples](https://github.com/themoretheless/lin/actions/runs/45997294522)
