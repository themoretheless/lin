# Current insert diagnostic phase profile

Source: 703bac5. Three independent processes, 24 fresh one-operation samples per size plus fixture validation/preflight: 27 phase events per size/process. Native macOS ARM, default embedding and scalar/FTS/identity/URI indexes retained. Builds/tests did not overlap measurement. Exact count/value fixture checks passed in all three complete runs.

| Documents | Row construction ms | Hash/ID/checks ms | Embedding ms | Scalar index ms | FTS ms | Row maps ms |
|---|---:|---:|---:|---:|---:|---:|
| 1000 | 0.207084 | 0.110208 | 0.298292 | 0.093417 | 0.134166 | 0.040458 |
| 10000 | 1.446250 | 1.313625 | 3.435333 | 0.919959 | 1.391167 | 0.558958 |

Construction includes initial row-vector allocation and (10k) caller+worker construction/join. Checks include batch-ID/URI set allocation/reserve, missing-ID/default-hash filling, duplicate/existing ID/URI and FK validation. Embedding includes text preparation, default batch embedding and vector insertion into rows. Remaining indexed phases are timed separately. Edge work, store reserve, moving rows, pack creation and Handle finalization are outside these timers.

These are medians within each process, then medians across three processes. Do not sum separate medians into a total or compare absolute times against independently timed peer benchmarks. Diagnostic clocks/logging, cache/load/CPU frequencies and compile changes may affect values. The older 2026-10-03 profile combined construction and checks; that column is not directly interchangeable with these separated columns. This is not proof of any new speedup or intrinsic CPU causality.

Embedding is the largest measured phase at both sizes. Next distinct experiment should examine repeated byte-trigram hashing while preserving exact vector bits and arbitrary-dimension/Unicode fallbacks. Lower row-worker thresholds already failed to improve small inserts, so repeating that tuning is not supported by the current evidence.

Temporary instrumented source and source/binary SHA256 are saved; production src/exec.rs restored byte-for-byte to HEAD BEFORE running the pinned diagnostic binary. Raw trace logs and benchmark observations retained. No production edits remain. The full nine-engine goal remains incomplete, including SQLite small/durable write losses and missing MSSQL/Kusto endpoint evidence.
