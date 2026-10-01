# Sparse WAL single-pass experiment — rejected

Compared pinned optimized baseline/candidate binaries in three alternating process pairs, 16 fresh fixtures per case, one operation per fixture. Exact rows validated outside timing; durable sync unchanged. No build/test process overlapped these measurements. Host load uncontrolled.

Candidate replaced the separate nonzero-count scan with an increment during serialization and backfilled the count. Wire representation unchanged.

| Pair | Lin 1k baseline ms | candidate ms | Lin 10k baseline ms | candidate ms |
|---:|---:|---:|---:|---:|
| 1 | 2.9367 | 2.6244 | 25.6875 | 25.1630 |
| 2 | 3.1445 | 2.9491 | 25.1734 | 26.6844 |
| 3 | 2.7148 | 2.6631 | 26.3431 | 26.6548 |

1k improved 3/3 pairs (median paired decrease 6.22%), while 10k improved only 1/3 (median paired decrease -1.18%). Unchanged SQLite controls varied substantially. No consistent benefit established for the larger workload; candidate reverted. Previous sparse codec and decoder bounds remain. Full raw observations and medians.json are alongside this report. Overall all-peer objective remains unproven.
