# Zero-marker embedding accumulator revisit: rejected

Baseline 518db365dd6accd80e86377baf18fedbca15429f after retained shared WAL vector storage. The October 3 zero-marker experiment improved native insertion but regressed durable 1k and was rejected. This revisit is a new combination on a materially changed durable path, motivated by ../2026-10-24-post-sharing-insert-profile/report.md identifying embedding as the largest native 1k measured phase. Old samples are not reinterpreted.

Candidate removes private Scratch.seen, using zero in the positive-weight accumulator as untouched marker. Hashing features remain positive quarter weights, unchanged sequence/hash/model/normalization/output bits. A debug assertion records the private positive-finite-weight requirement; finish resets touched coordinates and ensure_dim replaces the accumulator. Public/custom embedders and WAL format unchanged. Tests no longer initialize the deleted seen field; independent expected vector bits are unchanged.

Twelve focused embed tests pass, including original dense-reference bit comparisons, batch reuse, normalization threshold and new repeated-slot/dimension reset checks. Both pinned release binaries built. Six alternating process pairs per native/Full durable mode, 24 fresh fixtures per case, one operation/sample, no warmup. Existing exact fixture validation outside timing. No builds/tests launched by this runner overlap timing; other host activity/clocks uncontrolled. Positive gain means faster; wins count process medians.

| Case | Median paired gain | Wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.824% | 3/6 |
| compare/insert_bulk_1k/sqlite | +1.324% | 4/6 |
| compare/insert_bulk_10k/lin | -1.742% | 1/6 |
| compare/insert_bulk_10k/sqlite | -1.625% | 2/6 |
| compare/durable_insert_1k/lin | +3.164% | 4/6 |
| compare/durable_insert_1k/sqlite | +0.510% | 4/6 |
| compare/durable_insert_10k/lin | +1.492% | 5/6 |
| compare/durable_insert_10k/sqlite | +1.453% | 6/6 |

Reject without independent repeat/full workspace run: the targeted native 1k does not improve (3/6 wins), native 10k also slower, and the apparent durable benefit does not meet the intended combined goal. Durable 10k control improvement nearly matches Lin, so no isolated gain is claimed there. This does not establish a causal slowdown mechanism. Source embed.rs restored byte-for-byte; no production changes ship. Candidate/source/binary hashes, tests and raw observations preserved.

The original October 3 native gain did not reproduce on this source. Next work should investigate native row construction/validation or another independently justified hotspot, not retain a repeated failed hypothesis. Full named-peer objective remains open, including current SQLite gaps and remaining live peer coverage.
