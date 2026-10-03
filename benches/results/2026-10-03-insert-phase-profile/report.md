# Diagnostic native insert phase profile

Temporary bulk-insert instrumentation on fac9cd1. Three independent processes,
24 fresh one-operation samples per size; each phase records 27 calls per
process (observations plus harness validation/preflight). Fixture setup/drop
excluded. Native macOS ARM host; default embedding, scalar index, FTS and
identity/URI uniqueness retained. No builds/tests overlapped timing.

| Documents | Row build/checks ms | Embedding ms | Scalar index ms | FTS ms | Row maps ms |
|---|---:|---:|---:|---:|---:|
| 1,000 | 0.542542 | 0.400042 | 0.121833 | 0.178125 | 0.036083 |
| 10,000 | 5.677584 | 4.485208 | 1.192791 | 1.750625 | 0.416125 |

Row build includes record_row, default id/hash generation, batch/existing
identity/URI checks, foreign-key checks, and initial row/check-set allocation.
Embedding includes text preparation, batch embedding and insertion of vectors.
Scalar index, FTS and row-map registration are measured individually. Other
work (edge/reserve handling, moving rows, final pack creation and Handle work)
is outside these phase timers. Separate medians must not be summed to estimate
a total median. Host load/cache state and clock/log instrumentation affect
absolute times: these are diagnostic phases, not peer-speed proof.

Row formation is the largest recorded phase. FK checking is a short scan over
the catalog FK list without allocations; moving that check out of the loop
alone is unlikely to address the gap. Repeating the already-rejected
prepare-time sorted BTreeMap construction or shared WAL-vector changes is not
justified by this profile. Further construction work should investigate actual
allocation counts and storage representation while preserving exposed Row/API
behavior, rather than repeat scalar-loop changes without a distinct mechanism.

All three runs are complete with existing fresh-fixture readback/affected-count
gates. Instrumented benchmark builds. Production exec.rs restored byte-for-byte
before running the pinned executable. Source/binary SHA256, source copies,
raw trace logs and observations are retained. Trailing log blank lines normalized;
raw JSON unchanged. Full eight-engine goal remains unproven.
