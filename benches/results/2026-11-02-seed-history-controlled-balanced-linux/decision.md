# Controlled seed-history result — 2026-11-02

Run 41353571356 completed successfully. All 576 processes / 4608 observations, fixed seed values and embedding f32 bits, eight Linux setup traces, raw command/source audit and balanced six-role execution order verified. Local numerical reconstruction matches CI; all 3128 archive members rehashed. Linux executable payloads are not archived.

No window met six positive pairs above A/A (0/16). This diagnostic does not qualify an optimization. One same-binary null control exceeded the descriptive regression band: 10k-delete series2, -4.867% against 4.146% A/A. All results retained.

| Case | Grouped gain, median of 12 pairs % | Chunked Lin/SQLite | Grouped Lin/SQLite | Grouped wins |
| --- | ---: | ---: | ---: | ---: |
| 1k-update | -1.391 | 1.045895 | 1.076952 | 0/12 |
| 1k-delete | 0.436 | 1.099518 | 1.088992 | 1/12 |
| 1k-commit | -2.386 | 1.068777 | 1.082843 | 2/12 |
| 1k-rollback | 7.474 | 1.848134 | 1.684519 | 0/12 |
| 10k-update | 0.865 | 1.597136 | 1.575736 | 0/12 |
| 10k-delete | 2.186 | 1.627561 | 1.612264 | 0/12 |
| 10k-commit | 0.345 | 1.600779 | 1.583106 | 0/12 |
| 10k-rollback | -4.256 | 2.744687 | 2.773841 | 0/12 |

Grouping seed commits really changed setup WAL history (Lin 4/22 syncs at 1k/10k versus 3; SQLite 2 in both modes), while logical values matched. It also changes generation, allocation and cache history. The experiment does not identify a single physical latency cause. Mutation still uses one Lin fdatasync versus one SQLite fsync; rollback has neither.

Grouped preparation did not close the SQLite gap. At 10k rollback it was slower in both six-pair series (-5.510% and -3.513%). Keep the original benchmark fixture and retain this counterfactual as diagnostic evidence. No runtime adoption or eight-peer superiority claim.
