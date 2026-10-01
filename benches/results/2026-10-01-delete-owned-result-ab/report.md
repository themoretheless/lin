# Rejected owned delete-result experiment

Three independent process pairs, alternating order, 40 fresh fixtures per case.
Each measured operation mutates a fresh input, so iteration count is capped at one.
Timings are short; scheduler/cache/background effects limit precision.

| Pair | Table rows | Original Lin µs | Owned Lin µs | Original SQLite µs | Owned-run SQLite µs |
|---|---:|---:|---:|---:|---:|
| 1 | 1k | 5.500 | 6.396 | 3.292 | 4.542 |
| 1 | 10k | 14.959 | 14.209 | 10.583 | 12.375 |
| 1 | 100k | 22.084 | 18.979 | 19.146 | 16.250 |
| 2 | 1k | 5.604 | 3.833 | 2.500 | 2.583 |
| 2 | 10k | 11.312 | 10.438 | 8.666 | 6.500 |
| 2 | 100k | 20.250 | 20.958 | 15.604 | 17.062 |
| 3 | 1k | 6.792 | 8.999 | 4.042 | 5.854 |
| 3 | 10k | 13.687 | 15.229 | 7.562 | 12.646 |
| 3 | 100k | 23.480 | 27.041 | 18.230 | 17.250 |

The candidate did not show consistent gains and regressed all table sizes in
pair 3. It was removed. It passed workspace tests; a new regression for removed
row order and rollback positions remains. No SQLite deletion win is claimed.
Sources, hashes and raw runs are retained. The full multi-engine goal is unmet.
