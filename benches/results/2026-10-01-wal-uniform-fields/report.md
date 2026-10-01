# WAL uniform field-set fast path

Skip redundant BTreeSet insertions when a row has exactly the same ordered field names as the first row. Heterogeneous rows retain the full union path. Field names are still copied once into the owned pack; column inference, encoding, atomicity and fsync unchanged.

Six alternating process pairs, 24 fresh fixtures per case, one operation per fixture. Full exact-row validation outside timer. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled and case order fixed within a process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.825875 | 2.918687 | 24.548562 | 22.467834 |
| 2 | 2.970584 | 2.741208 | 23.399541 | 22.424687 |
| 3 | 2.947125 | 2.953187 | 23.685541 | 22.495833 |
| 4 | 2.958958 | 2.850708 | 24.200812 | 22.830354 |
| 5 | 3.027958 | 3.657458 | 24.290354 | 23.094896 |
| 6 | 2.863791 | 2.730396 | 23.711688 | 22.354000 |

10k candidate faster in 6/6 pairs; median paired decrease 5.34%. Unchanged SQLite controls also faster in 6/6, median decrease 1.99%; percentages are observed changes, not an isolated causal claim. 1k is mixed: 3/6 wins and median paired decrease 1.73%, with one marked regression. Retain for repeatable large-batch improvement, without claiming a small-batch speedup.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.884698 | 1.355688 |
| 10k | 22.481834 | 13.280458 |

Full offline workspace tests pass. Additional union test covers homogeneous rows, equal-sized rows with different keys, and later extra fields, verifying reconstructed values including existing missing-integer defaults. Scoped formatting and diff checks pass. Lin still loses these durable insert comparisons to SQLite; all-eight-peer objective remains unproven. Raw observations, baseline source and binary/source hashes retained.
