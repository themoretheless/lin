# Shared WAL vectors experiment — rejected

Internal ColData vector payload changed from owned Vec to Arc slices; writer packing and row reconstruction share Cell storage. Wire format unchanged, tested against legacy MessagePack bytes. Full workspace tests and a sharing/serialization test pass. The existing conservative decoded-vector budget remains unchanged.

Six alternating process pairs; first three use 16 fresh fixtures, next three 24. One operation per fixture; exact row checks outside timing; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap measurements; host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 3.065437 | 3.283917 | 27.054729 | 30.167333 |
| 2 | 3.799479 | 3.370167 | 34.973583 | 27.724021 |
| 3 | 3.601854 | 3.371125 | 30.802000 | 31.144270 |
| 4 | 3.586291 | 4.393750 | 29.418833 | 36.298541 |
| 5 | 3.100750 | 3.226646 | 26.834521 | 27.977521 |
| 6 | 3.670063 | 3.959833 | 33.243167 | 31.644646 |

Candidate wins 2/6 pairs at both sizes. Median paired duration changes: 1k +5.59%, 10k +2.69% (slower). SQLite controls varied substantially; no isolated causal slowdown asserted. Eliminated payload copies are verified, but sustained benchmark improvement is not. Candidate rejected and previous production source restored. Raw observations, candidate source and medians retained. Overall all-eight-peer goal remains unproven.
