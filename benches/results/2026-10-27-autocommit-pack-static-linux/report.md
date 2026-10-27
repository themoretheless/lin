# API-preserving Map/static-output candidate: Linux gate failed

GitHub run40448137140, frozen headfc3ecc3cf520d9c7c91fd762add4951b7f9cfca1, completed failure at the unchanged qualification gate. Baseline main047 Rust sources; candidate in-place Map Row/delta undo plus bounded bitmap prefix and compile-time autocommit Pack output recovery. Exact BTreeMap Row alias preserved; no COW wrapper. Tests and missing-output sensitivity passed.

Independent raw verification confirms720processes,5760observations,240paired comparisons, all expected groups/medians and reported candidate_qualified=false/full_objective=false. Nearby separate processes compare Lin and SQLite in each round; no peer CPU equivalence inferred. Two six-round series, each UPDATE target requires6/6positive and median gain>A/A; every unchanged Lin/SQLite control retains the original non-regression band. No rerun or discarded case.

| Lin case | Series | Gain % | Absolute A/A % | Positive | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/delete_1row_1k/lin | series1 | 1.631 | 3.349 | 5/6 | 0/6 |
| compare/delete_1row_1k/lin | series2 | 1.544 | 1.911 | 6/6 | 0/6 |
| compare/update_1row_10k/lin | series1 | 14.045 | 3.359 | 6/6 | 0/6 |
| compare/update_1row_10k/lin | series2 | 12.574 | 3.008 | 6/6 | 0/6 |
| compare/delete_1row_10k/lin | series1 | 4.736 | 1.830 | 6/6 | 0/6 |
| compare/delete_1row_10k/lin | series2 | 4.232 | 1.505 | 5/6 | 0/6 |
| compare/update_1row_100k/lin | series1 | 16.752 | 2.007 | 6/6 | 0/6 |
| compare/update_1row_100k/lin | series2 | 13.195 | 3.467 | 6/6 | 0/6 |
| compare/delete_1row_100k/lin | series1 | 5.625 | 3.535 | 6/6 | 0/6 |
| compare/delete_1row_100k/lin | series2 | 0.486 | 3.987 | 3/6 | 0/6 |
| compare/tx_commit_1row_1k/lin | series1 | 13.339 | 1.698 | 6/6 | 0/6 |
| compare/tx_commit_1row_1k/lin | series2 | 11.515 | 3.099 | 6/6 | 0/6 |
| compare/tx_rollback_1row_1k/lin | series1 | 12.666 | 1.156 | 6/6 | 0/6 |
| compare/tx_rollback_1row_1k/lin | series2 | 14.187 | 1.565 | 6/6 | 0/6 |
| compare/tx_commit_1row_10k/lin | series1 | 5.874 | 1.816 | 6/6 | 0/6 |
| compare/tx_commit_1row_10k/lin | series2 | 8.987 | 2.189 | 6/6 | 0/6 |
| compare/tx_rollback_1row_10k/lin | series1 | 8.314 | 1.828 | 5/6 | 0/6 |
| compare/tx_rollback_1row_10k/lin | series2 | 8.676 | 1.606 | 6/6 | 0/6 |
| compare/update_1row_1k/lin | series1 | 18.461 | 3.215 | 6/6 | 0/6 |
| compare/update_1row_1k/lin | series2 | 15.732 | 3.868 | 6/6 | 0/6 |

## Failed unchanged controls

| Case | Series | Gain % | Absolute A/A % |
|---|---|---:|---:|
| compare/delete_1row_1k/sqlite | series2 | -2.033863 | 1.647937 |
| compare/delete_1row_10k/sqlite | series1 | -2.008307 | 1.866263 |
| compare/delete_1row_10k/sqlite | series2 | -5.736332 | 0.971260 |
| compare/tx_rollback_1row_10k/sqlite | series2 | -4.363141 | 3.861817 |

All UPDATE targets improve6/6above A/A in both series, but four SQLite control groups fail. Independent candidate SQLite wins0/120 across all mutation/transaction comparisons. Native improvement over main is not peer superiority, and no qualified promotion follows. Do not round marginal control failures to pass. This candidate is rejected under the unchanged complete-run gate; unchanged retries are not warranted. Full durable/bulk/read/all-features/GPU/eight-peer qualification remains open.

Provenance correction: raw protocol.md inherited old COW descriptive text at the frozen CI head. It is preserved unchanged here as original artifact. Actual four candidate sources match intended Map/static source snapshots. Corrected protocol and explicit erratum copied alongside as protocol-corrected-after-run.md and protocol-erratum-verification.json; no input, timing or gate changed. This is documentation correction, not a changed experiment.

Separate embedding integration local native/durable gate also failed (../2026-10-27-embedding-reused-integration/report.md); it was not included in this Linux candidate. Do not mix their speed claims or peer counts.
