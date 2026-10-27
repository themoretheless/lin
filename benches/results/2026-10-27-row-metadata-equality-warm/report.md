# Conditional SoA refresh by exact values: rejected warm diagnostic

Baseline: previous Map/static-output candidate from Linux40448137140, not production main. Candidate changes only Store::row_maps_sync_updated: retain id/uri/stale-slot and length repair, compare actual resulting text values before replacing Arc columns, compare total float bits (including signed zero/NaN), invalidate packed title bytes only on changed title values or full rebuild. No patch-only assumption; external raw title changes still repair via value comparison. Generic public function signature unchanged.

Correctness:415 default library/integration tests passed,2existingignored, including existing prototype oracles/experimental mutation_contract target. New oracle540steps across docs/users/orders compares all relevant SoA columns and maps with independent full rebuild, exercises raw unsynchronized title edits, stale id maps, truncated column, nulls, identity changes and float0/-0/NaN/inf. Packed title validity separately checked. Missing wing refresh and missing title invalidation mutants each fail assertions, not compilation. Exact source/test snapshots and logs in ../2026-10-27-row-metadata-equality/.

18rotated A/A/B warm release processes,6rounds,1728observations: 16batches of512alternating updates and16explicit rollback observations per size/process. Readback/generation/count checks retained. Source restored; source/binary hashes retained.

| Rows | UPDATE gain % | Absolute A/A % | Positive pairs | Rollback gain % | Absolute A/A % |
|---|---:|---:|---:|---:|---:|
| 1000 | -5.906 | 3.061 | 0/6 | -3.739 | 3.624 |
| 10000 | -1.455 | 1.125 | 0/6 | -1.269 | 1.584 |
| 100000 | -3.562 | 1.550 | 0/6 | -2.267 | 1.936 |

Reject this performance substitution: every UPDATE pair negative at all sizes. This measures a combined value-comparison/conditional-refresh policy; it does not isolate a causal hardware mechanism. Do not repeat unchanged on Linux seeking a passing result. Correctness does not establish performance. Retaining still-valid packed title bytes across unrelated updates can retain cache memory longer than eager invalidation; no RSS/lifecycle advantage or performance win for a different mixed query workload was measured. No new default behavior or production promotion. No peer or eight-engine completion claim.
