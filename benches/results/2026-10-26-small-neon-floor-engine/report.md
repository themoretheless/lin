# Small NEON overlap: 512Ki float floor — not accepted

Frozen baseline `aa8f307f`, not the later stable-borrow-check compatibility changes.
Candidate extends shared persistent WAL encoding to 512..4095 rows with at least
512Ki finalized embedding floats; 4096..16384 retains its 2Mi minimum. Both use
an exact total and 16Mi upper bound. With default 768 dimensions the new entry
point is 683 rows. Full durability, format, indexes, FTS and input ownership remain
unchanged. Production policy was restored after the experiment.

## Functional evidence

Workspace/default, no-default and optional GPU/parallel/compression checks passed.
Replay/fallback covers 683/1000/4096, small exact budget 512 rows x1024 dimensions,
and serial guards 511,512,682/default,511x2048,4096x64,missing vectors,explicit
transactions and multi-statement calls. Encoder errors retain quota/index precedence.
24 process-exit writers and24 independent readonly verifiers cover512/683/1k/10k
x1/3/4 commits: exact rows/bits and byte-identical WAL in all12 scenarios.
Final release binary hashes match those measured. These are process-exit/replay
checks, not physical power-loss tests.

The initial draft accidentally changed the output quota while editing a numeric
literal. Workspace tests rejected it before measurements. Its source and failure
log are preserved as `failed-draft-*`; corrected snapshots restore the entire
original quota defaults. No production quota change was made.

## Conventional full-engine comparisons

Two series x6 alternating process pairs xnative/Full durable,24 fresh one-operation
samples per case:48 processes,4608 observations. SQLite controls retained. Setup,
validation and destruction outside timing; source/binary/compiler manifests saved.

| Case | Gain s1 / s2 (%) | Positive pairs s1 / s2 | Candidate ms s1 / s2 |
|---|---:|---:|---:|
| compare/durable_insert_10k/lin | -2.273 / -0.503 | 1/6 / 2/6 | 11.067 / 10.748 |
| compare/durable_insert_10k/sqlite | +0.370 / +0.509 | 4/6 / 4/6 | 14.340 / 13.696 |
| compare/durable_insert_1k/lin | +9.165 / +13.213 | 5/6 / 6/6 | 1.733 / 1.705 |
| compare/durable_insert_1k/sqlite | -4.207 / +2.607 | 2/6 / 4/6 | 1.474 / 1.400 |
| compare/insert_bulk_10k/lin | -1.862 / -16.807 | 2/6 / 2/6 | 8.340 / 29.804 |
| compare/insert_bulk_10k/sqlite | +0.085 / -55.542 | 3/6 / 2/6 | 10.507 / 22.144 |
| compare/insert_bulk_1k/lin | -2.630 / -0.025 | 1/6 / 3/6 | 0.792 / 1.036 |
| compare/insert_bulk_1k/sqlite | -0.389 / -2.042 | 2/6 / 0/6 | 0.803 / 1.085 |

## Interleaved controls

Two independent series,6 process pairs/size,24 alternating observations/variant.
A/A uses byte-identical baseline binaries. Complete samples, per-pair host load
and exact readback retained. Three families total6912 timed observations plus288
preflights. Native probes are a separate contract, not pooled with durable timings.

| Family / rows | A/B gain s1 / s2 (%) | Wins s1 / s2 | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| interleaved / 1000 | +12.913 / +11.895 | 6/6 / 6/6 | 2.209 / 4.064 |
| interleaved / 10000 | +0.892 / -0.640 | 5/6 / 2/6 | 1.104 / 5.594 |
| boundary / 682 | -1.162 / -3.165 | 1/6 / 2/6 | 3.267 / 1.658 |
| boundary / 683 | +10.726 / +11.804 | 6/6 / 6/6 | 1.435 / 1.412 |
| native / 1000 | +0.859 / +2.468 | 6/6 / 5/6 | 0.980 / 2.170 |
| native / 10000 | +0.231 / +0.581 | 4/6 / 5/6 | 0.799 / 0.657 |

## Decision and next hypothesis

Do not adopt this policy. Target1k gains repeat, but the below-budget682 guard
falls in both A/B series, second-series A/A is noisy, and conventional series2
has very large native10k and SQLite control shifts. Those observations do not
identify a cause. No samples/pairs are discarded. Candidate1k remains slower than
SQLite in the conventional durable comparison.

A distinct hypothesis is an O(1) conservative first-vector work hint before the
exact total scan: small slabs below the hint stay serial immediately, while all
worker-bound slabs still pass the exact total/upper budget. Mixed dimensions may
stay serial. It requires new snapshots from current main and full independent
qualification; this report does not validate that change.

`python3 <this-directory>/verify.py` checks frozen snapshot hashes, saved sample
medians/counts/statuses and replay/WAL evidence. It does not rerun the database or
turn historical timing into current-head proof.
