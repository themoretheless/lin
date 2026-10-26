# NEON sparse count: full-engine integration not accepted

Frozena5e28e36. Separate from the positive CPU-composition experiment. Candidate
only replaces both exact nonzero-count loops with bounded hardware-gated NEON
integer counts, plus unit tests. No codec/bytes/sync/quotas/index/embedding change.
The original production persist.rs has been restored byte-for-byte.

## Correctness and resource gates

Workspace/default, no-default and GPU/parallel/compression compile checks passed.
New scalar-oracle tests cover raw special bits, four pointer offsets and lengths
0..1024, plus2^20 accumulator boundaries through2Mi+31 floats. Existing decoder,
CRC/budget, worker fallback, quota/index precedence and empty-vector tests pass.
24 process-exit writers+24 readonly verifiers,512/683/1k/10k x1/3/4 commits:
exact rows/bits and byte-equal WAL in all12 scenarios. RSS/WAL sizes retained.
Final compare/interleaved release binary hashes match those measured.
This is not physical power-loss or other-architecture performance proof.

## Conventional pairs

Two series x6 alternating process pairs, native and Full durable1k/10k,
24 fresh one-operation samples/case:48 processes/4608 observations. SQLite
controls and every raw observation retained. Setup/validation/destruction outside
timing; embeddings/FTS/indexes and Full sync unchanged.

| Case | Paired gain s1 / s2 (%) | Positive pairs s1 / s2 | Candidate ms s1 / s2 |
|---|---:|---:|---:|
| compare/durable_insert_10k/lin | -0.119 / +0.517 | 2/6 / 4/6 | 9.960 / 9.839 |
| compare/durable_insert_10k/sqlite | +0.707 / +1.112 | 5/6 / 5/6 | 12.832 / 12.620 |
| compare/durable_insert_1k/lin | +10.878 / +5.597 | 5/6 / 5/6 | 1.982 / 2.036 |
| compare/durable_insert_1k/sqlite | +2.035 / -2.254 | 4/6 / 2/6 | 1.391 / 1.449 |
| compare/insert_bulk_10k/lin | -0.608 / -2.948 | 3/6 / 1/6 | 7.673 / 7.883 |
| compare/insert_bulk_10k/sqlite | -0.463 / -0.380 | 3/6 / 1/6 | 10.178 / 10.149 |
| compare/insert_bulk_1k/lin | -0.034 / -1.022 | 3/6 / 1/6 | 0.730 / 0.739 |
| compare/insert_bulk_1k/sqlite | -0.057 / -1.978 | 3/6 / 2/6 | 0.775 / 0.775 |

## Interleaved and boundaries

Two series per family,6 pairs/size,24 alternating samples/variant; A/A uses
byte-identical baseline binaries. Three families total6912 timed observations
plus288 preflights. Every operation validates all nine fields and f32 bits.

| Family / rows | A/B gain s1 / s2 (%) | Wins s1 / s2 | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| interleaved / 1000 | +5.532 / +1.875 | 6/6 / 5/6 | 1.484 / 3.222 |
| interleaved / 10000 | +0.069 / -0.470 | 4/6 / 2/6 | 1.584 / 0.476 |
| boundary / 682 | +1.748 / +2.585 | 4/6 / 5/6 | 4.186 / 2.493 |
| boundary / 683 | +4.037 / +5.775 | 5/6 / 5/6 | 2.576 / 2.651 |
| native / 1000 | +0.224 / +1.277 | 4/6 / 6/6 | 1.728 / 0.860 |
| native / 10000 | -1.220 / +0.093 | 2/6 / 3/6 | 0.490 / 0.414 |

## Additional matched A/control/B protocol

The second1k effect was close to A/A noise, so a distinct matched control was
added: each block has two byte-identical baseline processes and one candidate,
with all six request orders rotated.24 blocks across two sizes/two series,
1728 observations+72 preflights,72 worker processes. Baseline estimate is median
of the two baseline process medians. Raw observations and both baseline values
remain available; no control subtraction or outlier removal.

| Rows | Gain s1 / s2 (%) | Positive blocks s1 / s2 | Matched A/A absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| 1000 | +2.642 / +1.167 | 5/6 / 5/6 | 6.576 / 1.948 |
| 10000 | -0.963 / -1.009 | 1/6 / 1/6 | 1.649 / 0.594 |

The initial driver saved its first valid block and joined all three workers, then
failed its progress print because a variable shadowed Python round(). The driver
was fixed and resumed from block2; the first block was not rerun or overwritten.
Original driver/error log and resume notes are retained. This is a harness error,
not a database error, and no timing observations are discarded.

## Decision

Do not adopt the default integration. CPU composition improves≈28%, but matched
1k gains+2.64/+1.17% are below A/A spread6.58/1.95%;10k declines≈1% in both matched
series. These data do not prove the cause, a reliable commit win or a universal
regression. Positive conventional/interleaved results remain in full; they do not
override the later matched control. Candidate durable1k still loses to SQLite.

All source/tests/builds and raw evidence retained; source restored. A future
mechanism/workload change requires new evidence. Do not repeat this integration
unchanged or describe the CPU win as database acceleration. verify.py validates
saved hashes/medians/sample counts/replay evidence, not a new runtime run.
Rebuild scripts swap frozen files and require an isolated worktree.
