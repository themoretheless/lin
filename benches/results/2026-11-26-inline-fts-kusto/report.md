# Inline FTS candidate against Azure Data Explorer

Lin engine 0add1938e, benchmark revision 1a93715c2. Each workload ran in three fresh processes with three timing samples. The Lin API ran on Apple M4 Max. Kusto timings include the Azure round trip and result transfer. The inline ingestion timer includes visibility polling. The adapter validates values and multiplicity outside timing.

| Rows | Read wins | Closest read ratio | Insert wins | Lin insert ms | Kusto insert ms |
|---:|---:|---:|---:|---:|---:|
| 1000 | 18/18 | 1718.02× | 3/3 | 0.871 | 913.238 |
| 10000 | 18/18 | 771.50× | 3/3 | 7.204 | 1760.421 |
| 100000 | 18/18 | 255.06× | 3/3 | 105.837 | 26204.949 |

## Reads at 100k

Median of the three process medians, milliseconds.

| Query | Lin | Kusto |
|---|---:|---:|
| filter_eq | 0.000253 | 286.881 |
| join_filter | 4.077083 | 7211.823 |
| join_inner | 9.216833 | 17536.369 |
| materialize | 0.377897 | 5839.439 |
| point_get | 0.000029 | 198.853 |
| text_substr | 0.207524 | 326.675 |

The second 100k INSERT process reported ResponseNotReady and a cleanup error. Its entire pair was replaced with a fresh Kusto-first process on the same binary. Fixture cleanup for the failed connection was not confirmed.

The 1k read run flagged a working-tree change to the peer coverage Markdown file. All runs used the same verified Lin binary. [Timing samples](samples.json)
