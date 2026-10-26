# Patch-driven undo allocation and rollback control

Frozen main205 vs patch-undo30ef20ce. Three independent processes per binary,12 fresh-fixture operations per process (wing/title/delete/rollback at1k/10k/100k),72 operations total. Exact rows, scalar index readback and baseline generation behavior checked outside allocator counter. Setup/readback excluded; Prepared::run, Handle drop, undo drop and explicit transaction rollback restoration included. Timestamp fixture is fixed; diagnostic does not replace fresh-fixture timing matrix. Production source restored exactly; source/binary SHA retained.

| Case | Allocation requests before -> after | Requested bytes saved |
|---|---:|---:|
| UPDATE wing | 71 -> 62 | 514 |
| UPDATE title | 85 -> 76 | 513 |
| DELETE | 71 -> 71 | -64 |
| UPDATE + explicit rollback | 158 -> 109 | 2842 |

Identical counts/byte deltas in all three processes and all sizes. Negative DELETE memory control retained: larger RowUndo enum increases the event vector allocation by64bytes even though Delete payload/logic is unchanged. This is not a no-regression memory claim; multirow capacity and peak RSS remain unmeasured. Counters measure allocation/reallocation requests, not retained memory or total RSS.

Positive allocation results do not prove latency or SQLite superiority. Linux run40418391258 is the frozen full-engine A/A/B timing qualification, still running when this diagnostic was saved. No production rollout or all-eight-peer completion established.
