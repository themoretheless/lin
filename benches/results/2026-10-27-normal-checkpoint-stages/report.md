# Normal-mode checkpoint stage diagnostic

Baseline/control use the same original binary; disabled/traced use the same instrumented binary.3 fresh processes per role,1000 growing transactions each,5 writes/transaction. SQLite WAL NORMAL retains its1000-page autocheckpoint; Lin retains32-commit checkpoints and every existing sync call. Full input-value reopen passes in all15 processes. This is a local macOS diagnostic, not optimization qualification.

| Repeat | Phase | Median ns | Median fraction of checkpoint % |
|---:|---|---:|---:|
| 1 | fence | 46292.0 | 0.252 |
| 1 | wal_sync | 675125.0 | 3.405 |
| 1 | prepare | 709.0 | 0.004 |
| 1 | snapshot | 6301750.0 | 33.341 |
| 1 | head | 5554375.0 | 30.659 |
| 1 | fts | 1080666.0 | 5.993 |
| 1 | compact | 4620583.0 | 25.424 |
| 2 | fence | 43875.0 | 0.250 |
| 2 | wal_sync | 556875.0 | 3.346 |
| 2 | prepare | 792.0 | 0.004 |
| 2 | snapshot | 6085708.0 | 33.492 |
| 2 | head | 5444667.0 | 30.808 |
| 2 | fts | 1064125.0 | 6.197 |
| 2 | compact | 4430208.0 | 25.461 |
| 3 | fence | 39375.0 | 0.220 |
| 3 | wal_sync | 552625.0 | 3.107 |
| 3 | prepare | 792.0 | 0.005 |
| 3 | snapshot | 5654666.0 | 33.870 |
| 3 | head | 5425917.0 | 30.183 |
| 3 | fts | 986125.0 | 5.442 |
| 3 | compact | 4522417.0 | 25.585 |

Fractions are computed per automatic checkpoint then medians within each process. Whole-function timing is inclusive; do not add it to nested stages. Snapshot includes encoding,file sync,rename and directory sync. FTS includes all cache writes. Fence includes acquisition; compact includes truncate,seek and sync_all. Those composite spans do not isolate individual syscalls. Final close/reopen checkpoints are preserved in raw logs but excluded from the automatic-checkpoint table.

| Repeat | Control/original | Disabled/original | Traced/disabled | Original/SQLite |
|---:|---:|---:|---:|---:|
| 1 | 0.9859 | 0.9692 | 1.0669 | 33.2506 |
| 2 | 1.0075 | 1.0350 | 0.9754 | 32.1565 |
| 3 | 1.0319 | 0.9911 | 1.0235 | 31.8336 |

Observer ratios use whole transaction mean within each fresh process. Counter reads,clock overhead,code layout and trace serialization/stderr inside transaction time are not calibrated away. Whole stage timing stops before trace JSON emission,while the transaction timer includes emission. Ratios from this small three-process diagnostic are descriptive,not a source gate or cross-engine speed claim. All raw losses are retained. Production sources were restored; hashes and both executable identities are retained.
