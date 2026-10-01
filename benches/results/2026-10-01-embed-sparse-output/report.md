# Sparse embedding output experiment — rejected

For sparse Scratch.finish outputs, initialize a new Arc slice with +0 and copy only touched coordinates; dense output path unchanged. Normalization and feature generation unchanged. Public Embedder API already returns Arc slices and was not changed.

Six alternating process pairs; 24 fresh fixtures per case, one operation. Exact rows checked outside timing, Full fsync unchanged. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.934292 | 2.889291 | 21.737958 | 22.491458 |
| 2 | 2.516230 | 2.768541 | 21.642459 | 21.981125 |
| 3 | 2.633979 | 2.657896 | 21.953771 | 21.404438 |
| 4 | 3.701145 | 2.583500 | 21.042792 | 21.325480 |
| 5 | 2.636938 | 2.645104 | 20.835938 | 21.737125 |
| 6 | 2.664521 | 2.729771 | 21.657979 | 21.790875 |

Candidate faster only 2/6 pairs at 1k, 1/6 at 10k. Median paired decreases -0.61% and -1.45% (slower). Unchanged SQLite controls varied; no isolated causal slowdown asserted. Candidate rejected, original embedding implementation restored. Previous WAL optimizations retained.

Full offline workspace tests pass, including bit-exact reference comparisons and normalization boundary cases. Formatting and diff checks pass. Raw observations, candidate and baseline source and binary hashes retained. All-eight-peer goal remains unproven.
