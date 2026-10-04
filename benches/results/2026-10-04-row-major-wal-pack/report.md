# Row-major WAL column packing

Retained final implementation: for uniform batches of at least 4096 rows whose first row has no null cells, allocate typed columns from that first row and fill all columns in one traversal of each row. This removes the temporary flattened cell-reference buffer and repeated column traversal. Smaller, heterogeneous and null-first batches retain the existing conversion path. Wire format, field ordering and type coercions are unchanged.

Baseline: main 525b257261b6394307386f70db25f538f9ab8e99. Source snapshots and SHA-256 metadata distinguish the initial unrestricted candidate from the final size-gated candidate. Executables were pinned before timing; no builds or tests overlapped timing.

Each series consists of six alternating baseline/candidate process pairs. Each case uses 24 fresh fixtures, one operation per sample, no warmup. Reported gains are the median of the six paired percentage improvements, not the ratio of aggregate medians. SQLite is an unchanged control run in both executables. These timings apply to the existing local compare durable fixture and are not universal engine CPU measurements.

| Series | Lin durable 1k | SQLite control 1k | Lin durable 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial unrestricted | -0.597%, 3/6 | +1.213%, 4/6 | +1.292%, 5/6 | +0.247%, 4/6 |
| Independent unrestricted repeat | -0.693%, 2/6 | +1.649%, 4/6 | +3.144%, 6/6 | +1.009%, 4/6 |
| Final size gate | +0.087%, 3/6 | -0.951%, 2/6 | +1.885%, 6/6 | -0.247%, 1/6 |

The unrestricted implementation slightly regressed the small fixture twice, motivating the final size gate. The final large fixture consistently improved, while the small fixture is effectively unchanged. The 4096 threshold is a conservative gate, not a measured optimal crossover.

Final aggregate process-median timings: Lin 1k 3.240365 -> 3.119396 ms (paired gain is only 0.087%); Lin 10k 23.005239 -> 22.546396 ms. Final SQLite medians were 1.801646 ms and 18.331562 ms respectively. Lin still loses these durable write comparisons to SQLite; the full named-peer objective remains incomplete. This change does not establish MSSQL or Kusto performance.

Validation: all seven focused WAL integrity tests passed; full workspace tests passed for the final size-gated implementation. A new 4096-row regression checks exact serialized frame bytes against explicitly constructed columns, including null/default coercions, integers in float/time columns, negative zero, NaN payload and infinity. Existing field-union, shared text and replay tests passed. The initial test failure in tests-initial.log came from the frame test helper appending the expected frame after the actual frame; resetting the test file before comparison fixed the test setup. No production correction was needed for that failure.

Raw results: medians.json, repeat-medians.json, final-medians.json and per-process run.json/log files. run-pairs.py, run-repeat.py, run-final.py reproduce the benchmark invocation using the pinned executables; source/binary hashes are saved separately for the final variant.
