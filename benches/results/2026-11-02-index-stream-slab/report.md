# Index slab streaming: no demonstrated local gain

Removed temporary matching-row and key vectors from LiveIndex::insert_slab_batch. Kept row order, captured timestamp, partial/unique semantics and bitmap sampling/thresholds. Independent reference passed; complete default regression 463 passed / 2 ignored; unchanged worker 100k quota, default embedding and exact-row oracle passed.

54 fresh local worker processes, 432 operation samples, 1998000 exact six-field row readbacks. Three roles counterbalanced over six rounds; base/control use the identical baseline executable. Baseline detached worktree b88ebea2e and candidate source/binary fingerprints, commands, compiler/host, raw data and all negative pairs retained. No builds were running during measurement. Independent recomputation verified 108 raw members and all numeric summaries.

| Rows | Median latency reduction | AA control variation | Positive pairs |
|---|---:|---:|---:|
| 1k | +2.060% | 6.094% | 4/6 |
| 10k | -2.150% | 2.616% | 1/6 |
| 100k | +3.732% | 7.452% | 4/6 |

No uniform improvement above control variation. In particular the 100k result cannot establish progress against DuckDB. Candidate not adopted; runtime src/index.rs restored byte-for-byte to baseline, candidate retained as snapshot and in commit 234ced56c. Local mixed results do not establish x86 Linux effects or compiler causes. No peer superiority, full shared-matrix acceptance or goal completion is claimed. No unchanged retry or threshold waiver.
