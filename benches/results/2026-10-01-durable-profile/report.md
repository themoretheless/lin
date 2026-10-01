# Durable insert profile and borrowed WAL field names

## Profile

Sampling example profile_durable ran 210 fresh 10k-document batches in 25.071 s. Timed insert calls totaled 6.823 s; setup 3.368 s. The remaining time includes validation, DB close/checkpoint, destruction and temporary-directory cleanup. This diagnostic is not a peer benchmark. sample.txt contains the full 15-second macOS stack capture.

The rows_to_insert_cols subtree contains BTreeSet<String> insertion, memcmp, allocation and free stacks. Existing code cloned every field name for every row, including duplicates. Changed the temporary union to BTreeSet<&str> and copy each unique field name only once into the owned pack. Field union, order, column inference, wire encoding and durability remain unchanged.

## Paired benchmark

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Exact row validation outside timer, Full fsync unchanged. Fixed per-process case order. Pinned optimized binaries; no build/test overlapped measurement. Host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 3.130791 | 2.570229 | 25.592875 | 24.452334 |
| 2 | 2.747416 | 2.607604 | 26.308813 | 23.823708 |
| 3 | 2.763417 | 2.545167 | 25.066063 | 24.835355 |
| 4 | 2.770459 | 2.576000 | 26.405729 | 23.757770 |
| 5 | 3.067541 | 3.591625 | 28.507458 | 60.798146 |
| 6 | 3.405958 | 3.321646 | 26.151521 | 31.751187 |

Observed Lin duration decreased in 5/6 pairs at 1k (median paired decrease 6.05%) and 4/6 at 10k (median paired decrease 2.69%). Pair 5 has a large 10k regression; pair 6 also regresses. Unchanged SQLite controls varied substantially and mostly slowed. These percentages are observations, not isolated causal attribution or significance claims. Retain the allocation-removing change provisionally; larger-workload consistency remains unresolved.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.591802 | 1.400771 |
| 10k | 24.643844 | 15.077678 |

Lin still loses these durable comparisons to SQLite. This does not prove superiority over the full eight-peer objective.

Full offline workspace tests pass. Scoped rustfmt and git diff checks pass. Raw run.json observations, benchmark logs, pre-change source, profile and binary/source hashes retained.
