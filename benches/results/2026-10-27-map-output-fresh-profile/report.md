# Actual Map/static-output mutation candidate: fresh-fixture phase profile

Source snapshots exactly the API-preserving candidate from failed Linux run40448137140, not COW and not production main. Uses existing checked compare fixture and real update/delete callbacks. Default indexes/FTS/embeddings/full Handle/drop retained. Three macOS release processes per kind,1k/10k/100k,8 measured samples per case:144 wall observations;198 complete phase records include framework validation/calibration. Source restored and hashes checked; binary/probe/instrumented source snapshots retained.

| Case | Prepared us | Output-row clone us | Scalar maintenance us | Row maps us | Position lookup us |
|---|---:|---:|---:|---:|---:|
| update/1000 | 1.917 | 0.167 | 0.292 | 0.250 | 0.084 |
| update/10000 | 6.250 | 0.334 | 1.000 | 0.917 | 0.833 |
| update/100000 | 9.875 | 0.500 | 2.083 | 1.375 | 1.042 |

| DELETE case | Prepared us | Store delete us | Tail move us | FTS move us |
|---|---:|---:|---:|---:|
| delete/1000 | 3.625 | 2.167 | 1.083 | 0.792 |
| delete/10000 | 9.958 | 6.500 | 3.000 | 2.000 |
| delete/100000 | 16.542 | 11.208 | 5.750 | 3.875 |

Instrumentation is diagnostic only and changes cost; nested medians do not sum and cross-platform/source profiles cannot be compared causally. Output copy is a small measured portion here, not proof of the fundamental full-performance limit suggested before profiling. Scalar index and row-map maintenance deserve the next investigation. Row-map sync currently validates id/uri and slot identity and refreshes all collection-specific SoA columns even for an isolated wing update. Any selective maintenance must preserve stale-map repair for public Store mutations, identity changes, packed-title invalidation, typed query results, rollback/replay and complete query equivalence. No implementation or speedup inferred for that hypothesis.

DELETE remains dominated within Store by tail relocation/FTS. The previous known-empty FTS variant already lacked a stable full measured gain; do not retry it unchanged.

Premeasurement correction retained: initial generic function-name prefix matched exec_program_readonly rather than const-generic exec_program; no measurement ran with it. Corrected exact-name/generic regex, rebuilt, verified stage15 at actual exec_program before timing. Initial build/provenance/instrumented exec archived under premeasurement-prefix-probe-*. Raw phase JSON equals parsed PHASE lines; all expected records verified. No peer win, production rollout or eight-peer completion.
