# Retained FTS append fast path

Three independent process pairs, alternating order; 12 fresh fixtures per case.

| Pair | Original full 10k | FTS append | Improvement |
|---|---:|---:|---:|
| 1 | 14.224 ms | 13.764 ms | 3.2% |
| 2 | 13.828 ms | 12.654 ms | 8.5% |
| 3 | 12.887 ms | 12.305 ms | 4.5% |

Median paired improvement: 4.5%.
The candidate won all three full-insertion pairs and is retained. Background
load is uncontrolled; this is local evidence, not a universal peer win.
Fresh slabs without pending deltas append directly; recycled positions with
pending deltas use the general path. Tests cover Unicode, duplicate words,
tail recycling, fold parity and durable delete/checkpoint/reopen behavior.
Both source variants, hashes and raw runs are saved under this directory.
