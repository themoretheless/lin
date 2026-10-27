# COW in-place full-operation allocation controls

Three processes per variant,24fresh-fixture operations each:144operations,total72pairs. Sizes1k/10k/100k;wing/title/delete/rollback;unique and held_snapshot. Old expected row is built by iterating fields into an independent Row outside timing/counter,so validation itself does not share the stored map. held_snapshot explicitly retains an additional real Row clone. Counts include Prepared/Handle/undo/drop and transaction rollback,exclude setup and readback. Exact scalar-index and row restoration/isolation checks pass in all operations.

| Case | Baseline | Unique candidate | Held snapshot candidate |
|---|---:|---:|---:|
| Wing UPDATE | 71 | 12 | 23 |
| Title UPDATE | 85 | 31 | 42 |
| DELETE | 71 | 41 | 41 |
| UPDATE+rollback | 158 | 50 | 61 |

Identical counts across sizes/processes. Requested-byte differences retained in summary.json including all controls. No RSS,latency or peer claim. Held-snapshot overhead is required isolation and not excluded. Input construction differs from earlier counters that unintentionally kept the old COW Row shared; old artifacts retained and not reinterpreted as unique-state evidence. Source/binary hashes verified,production source restored exactly. API migration and full-engine/Full durable/read/memory qualification remain open.
