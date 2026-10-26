# Refined mutation stage diagnostic

Main205 production source, temporary instrumentation only, release macOS ARM64. Three independent processes, nine fresh-fixture operations each, all 27 exact readback/index checks pass. Source restored byte-for-byte. Raw logs retain first-invocation outliers. Allocation counting and Instant instrumentation perturb measurements; these are inclusive nested spans and must not be summed. No baseline/candidate speed qualification is claimed.

Median DELETE stage nanoseconds:

| Rows | delete_rows | Store deletion | Tail maintenance | FTS move | Scalar index move | Column swap | Identity-key removal |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1000 | 10416 | 9000 | 3500 | 1625 | 250 | 1000 | 459 |
| 10000 | 10500 | 7417 | 3417 | 1959 | 209 | 333 | 583 |
| 100000 | 13250 | 9500 | 4417 | 3042 | 375 | 334 | 458 |

FTS maintenance is a material portion of the tail step, while column movement is smaller at 10k/100k. This supports investigating removal of a known maximum row position directly from base postings, avoiding new pending tombstones where the base last element proves membership. It does not justify shipping the rejected fused FTS v1 candidate. Correct handling of pending additions overlapping base, pending deletions, recycled positions, shared tokens, folds and missing-base cases must precede another full-engine experiment.

The fixture matches title token shape and CAS mutation path but uses fixed timestamps, and only macOS diagnostics were run. This is not the Linux acceptance matrix or proof of superiority over SQLite or the other seven peers. summary.json includes all update/title/delete stages, including negative or outlier observations in raw logs.
