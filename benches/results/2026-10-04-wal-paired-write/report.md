# Paired sparse WAL writes: rejected

Baseline: 3f5e556. Candidate packs index-u32 and raw f32 bits into one little-endian u64 append rather than two u32 appends. Exact byte order, negative-zero/NaN payloads, sparse row selection, codec, checksums and fsync guarantees remain unchanged. Both sources and binary hashes saved.

Each series uses six independent alternating baseline/candidate process pairs, 24 fresh one-operation samples per case/process, no warmup. Preparation/schema/setup excluded. Exact fixture/count/value/readback checks passed. No builds/tests overlapped timed measurements.

Initial mixed-size run

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -0.40% | 3/6 |
| compare/durable_insert_1k/sqlite | -7.54% | 0/6 |
| compare/durable_insert_10k/lin | -20.59% | 1/6 |
| compare/durable_insert_10k/sqlite | -22.66% | 0/6 |

Independent 10k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_10k/lin | 2.49% | 5/6 |
| compare/durable_insert_10k/sqlite | -1.84% | 2/6 |

Independent 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -2.47% | 1/6 |
| compare/durable_insert_1k/sqlite | -0.02% | 3/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). The initial mixed-size run is strongly confounded by unmodified SQLite controls slowing in all six pairs, especially 10k. Those raw data are retained, not discarded. Independent single-size repeats were run with the SAME pinned binaries to resolve that uncertainty, without rebuilding or changing the candidate.

The 10k repeat improved Lin 2.49% in 5/6 pairs while SQLite control slowed 1.85%. The 1k repeat regressed Lin 2.47% in 5/6 pairs while SQLite control stayed essentially flat (-0.02%). Decision: reject this small-write tradeoff. The objective includes the unresolved small SQLite gap, and this variant worsens it in the better-controlled repeat. Whole-operation wall times do not establish an intrinsic CPU cause for the regression.

Two focused sparse-vector integrity tests passed (exact -0/NaN/Inf bits, missing/empty vectors, partial tails/block boundaries, dense fallback, checksums/truncation and malformed bounds). Both benchmark binaries built. Full workspace tests were not run for this rejected candidate. Production src/persist.rs restored byte-for-byte to baseline. Log trailing blank lines normalized; raw JSON unchanged. `git diff --check` passed. No production code changes remain.

The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint proof. This is a rejected experiment with follow-up evidence, not a universal acceleration or full peer matrix refresh.
