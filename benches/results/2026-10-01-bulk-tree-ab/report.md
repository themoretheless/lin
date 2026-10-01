# Rejected sorted scalar-index slab construction

Three independent process pairs, alternating order, 12 fresh fixtures per case.

| Pair | Original full 10k | Sorted bulk build |
|---|---:|---:|
| 1 | 12.768 ms | 14.030 ms |
| 2 | 12.893 ms | 13.826 ms |
| 3 | 13.287 ms | 13.720 ms |

Process-median aggregate: 12.893 ms original vs 13.826 ms sorted.
The candidate lost all three full-insertion pairs and was removed. It passed
the workspace suite and direct parity/uniqueness tests. Sources, hashes and raw
runs are saved. No peer win was established. Background load is uncontrolled.
