# Shared text in WAL column packs

Internal ColData::Text uses Arc<str>, sharing existing Cell text when constructing a WAL pack and reconstructing rows. The temporary pack no longer allocates a String for every text cell. Wire encodings and default values unchanged; missing text still becomes an empty string. Public API unchanged (persist module private).

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Full exact-row validation outside timer; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap timings. Host load uncontrolled and fixed case order within a process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.806792 | 2.767709 | 23.900250 | 22.800896 |
| 2 | 2.925458 | 2.602208 | 23.340312 | 22.575875 |
| 3 | 3.000584 | 2.597167 | 23.293521 | 21.708230 |
| 4 | 4.148854 | 2.939833 | 23.831312 | 22.403604 |
| 5 | 3.309125 | 2.729376 | 23.660604 | 22.970917 |
| 6 | 2.756417 | 2.710876 | 23.421208 | 22.635000 |

Candidate faster in 6/6 pairs at both sizes. Median paired decrease 12.25% at 1k and 3.98% at 10k. Unchanged SQLite controls also improved by median 5.96% and 2.13%, with a large outlier. Observed percentages are not isolated causal attribution or statistical significance claims. Retain the change.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.720125 | 1.425000 |
| 10k | 22.605438 | 13.954667 |

Full offline workspace tests pass. Added compatibility test verifies byte-identical MessagePack versus legacy Vec<String>, legacy decoding, empty/Unicode/NUL text, exact reconstructed rows and shared Arc identity. Scoped rustfmt and git diff checks pass. Lin still slower than SQLite in these durable inserts; no all-eight-peer victory established. Raw observations, baseline source and hashes retained.
