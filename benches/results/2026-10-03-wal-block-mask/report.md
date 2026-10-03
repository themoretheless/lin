# Sparse WAL block masks: rejected

Baseline: 2651115. Candidate replaces the eight-element bitwise-OR empty-block check followed by conditional element traversal with an eight-bit presence mask and trailing-zero traversal. Negative zero and all NaN payloads remain present, ascending index order is preserved, and wire format/sync guarantees are unchanged. Saved sources and binary hashes identify the actual measured experiment.

Six alternating independent baseline/candidate process pairs; 24 fresh fixtures per process, one operation/sample, no warmup. Setup/schema/preparation excluded; synchronous operation including durability flush measured. Exact fixture/readback checks passed. SQLite controls measured alongside Lin. Compilation/tests did not overlap measurement.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/durable_insert_1k/lin | -1.69% | 2/6 | 2.520719 | 2.608083 |
| compare/durable_insert_1k/sqlite | 1.27% | 4/6 | 1.423688 | 1.419823 |
| compare/durable_insert_10k/lin | 0.65% | 4/6 | 19.073958 | 19.005094 |
| compare/durable_insert_10k/sqlite | 0.94% | 4/6 | 14.163094 | 14.029719 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). The small durable workload regressed; the larger workload gain is weaker than SQLite control movement. Decision: reject. src/persist.rs was restored byte-for-byte to baseline. This experiment does not close the existing durable-write gap against SQLite.

Validation: two focused sparse-vector library tests passed, covering bit-preserving -0/NaN/Inf values, missing/empty vectors, block boundaries, partial tail, dense fallback, checksums, truncation, and malformed dimensions/indices/trailing bytes. Both benchmark binaries built. Full workspace tests were not run for this rejected candidate.

First launches waited in macOS dyld before application entry; first baseline startup stack was captured in startup.sample. The existing live benchmark handle was retained throughout, without duplicate/restarted measurement processes. Startup delay did not enter benchmark samples. All twelve benchmark processes finished successfully with complete reports. Raw samples are retained; batch averages are not independent process replications or latency percentiles.

MSSQL/Kusto endpoint variables remain absent. The full nine-engine goal remains incomplete, independent of this rejected local optimization.
