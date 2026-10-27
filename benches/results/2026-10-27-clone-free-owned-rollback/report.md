# Clone-free owned-row rollback: functional prototype

Distinct from the previously rejected owned rollback, which retained one cloned restored BTreeMap for maintenance. This private helper owns the incoming undo Row, temporarily takes the current Row out of its slot, removes current scalar/FTS entries, inserts incoming entries by borrowing the still-owned undo Row, updates direct row-map columns and moves the incoming Row into Store. The common path makes no complete Row clones in the helper. Public Row remains the exact BTreeMap alias; public restore_rows remains unchanged. Both Rows and Mixed internal undo branches call the new helper.

The rare identity-change, stale-identity-slot or short column-cache cases first install the restored Row and then rebuild its row maps. This preserves the requirement that rebuild sees a populated Store slot. The direct path duplicates the existing rebuild preconditions explicitly; future changes to row_maps_sync_updated must keep them synchronized. Scalar index insertion errors retain the existing ignored-error rollback behavior; no broader rollback atomicity, optional HNSW behavior or arbitrary panic-safety claim is made.

Complete default release library/integration suite:433 passed,2ignored. Dedicated regression covers id/uri/wing restoration and exact full canonical rows after repeated updates, scalar-index results and unchanged committed generation. Existing WAL, transactional error, mixed rollback and recovery tests pass. Original sources and temporary regression are restored/removed in finally. Candidate snapshots are retained only as an experiment.

No performance qualification yet. Previous owned rollback improved rollback while failing unchanged UPDATE controls; this revision must measure both controls and target, including ordinary Handle/undo/drop costs, before any full fresh-fixture/native/durable/read gate. No all-eight-peer claim follows from functional success.

## Warm paired diagnostic: gate not passed

Two series of six rounds,36 fresh processes,3456 metric observations (16 samples ×3 sizes ×2 metrics per process). Baseline/control share the exact binary; candidate is separately built. Global counting allocator from the historical harness was removed; both variants use their ordinary allocator. Full target row is canonical-serialized and compared to the seed outside timing after each UPDATE block and each rollback; indexed counts and committed generations are independently checked. UPDATE blocks include512 alternating prepared writes and returned Handle destruction. Rollback timer includes UPDATE, undo/Pack construction, rollback and drops. This is repeated-row CPU evidence, not fresh-fixture Linux, Full sync, peer or memory acceptance.

| Rows | Metric | Series | Gain % | Absolute A/A % | Positive | Pass |
|---:|---|---|---:|---:|---:|---|
| 1000 | rollback_ns | series1 | 23.327 | 4.485 | 6/6 | True |
| 1000 | rollback_ns | series2 | 23.306 | 2.044 | 6/6 | True |
| 1000 | ns_per_update | series1 | 1.262 | 1.958 | 5/6 | True |
| 1000 | ns_per_update | series2 | 0.992 | 0.913 | 4/6 | True |
| 10000 | rollback_ns | series1 | 24.586 | 2.250 | 6/6 | True |
| 10000 | rollback_ns | series2 | 22.866 | 1.450 | 5/6 | False |
| 10000 | ns_per_update | series1 | 0.865 | 2.147 | 5/6 | True |
| 10000 | ns_per_update | series2 | 1.751 | 0.976 | 5/6 | True |
| 100000 | rollback_ns | series1 | 21.917 | 1.007 | 6/6 | True |
| 100000 | rollback_ns | series2 | 24.460 | 4.151 | 6/6 | True |
| 100000 | ns_per_update | series1 | -0.542 | 1.151 | 3/6 | True |
| 100000 | ns_per_update | series2 | 1.935 | 1.097 | 6/6 | True |

All unchanged UPDATE control windows pass. Rollback1000/100000 pass both series; rollback10000 fails series2 consistency with5/6 positive pairs despite median22.866% gain above1.450%A/A. In round7, baseline/control/candidate medians are5062.5/5021/5708.5ns (candidate -12.760%). All16 samples remain; the candidate interval contains several larger observations. Load records do not prove their cause. This pair is not removed or reclassified. Diagnostic gate=false; production adoption remains unqualified, and no identical warm rerun is launched to seek a pass. Initial analyzer syntax error is retained; correcting its token spacing did not regenerate measurements.
