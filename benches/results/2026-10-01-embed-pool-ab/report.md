# Reusable embedding pool: not retained

Three independent process pairs; 12 fresh-fixture samples per case. Times are medians in milliseconds.

| Pair | Serial full | Pool full | Serial no embedding | Pool no embedding |
|---|---:|---:|---:|---:|
| 1 | 13.744 | 13.812 | 8.954 | 10.000 |
| 2 | 17.809 | 29.664 | 79.140 | 13.140 |
| 3 | 51.878 | 19.100 | 22.987 | 15.975 |

The pool did not demonstrate a consistent win. Severe control variation prevents attributing the differences to the implementation. The candidate was reverted, including its direct Rayon dependency. The existing serial embedding optimizations and WAL/count correction remain.

Pool measurements follow benchmark validation/calibration and therefore measure a warm reusable pool. These results do not establish cold first-use latency, equal CPU cost, or wins against other engines. Source variants and raw observations remain in this directory.
