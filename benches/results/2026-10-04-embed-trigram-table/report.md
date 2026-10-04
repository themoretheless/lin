# ASCII trigram embedding table: rejected

Baseline: 5a6937b. Candidate computes 26^3 ASCII lowercase-letter trigram slots using the active FxHasher/slice Hash implementation for dimension 768; other triples/dimensions use the original hash path. Feature order/weights/normalization and vector bits remain unchanged. The additional table payload is 35,152 bytes (about 34 KiB), with one-time initialization in the first default embedder constructor. Other-dimension paths also compute range indices before their fallback in this measured candidate. No model identity or wire-format change.

Seven focused embedding tests passed, including exhaustive slot checks for all 17,576 letter triples and original dense-reference f32-bit comparisons across dimensions and ASCII/Unicode/empty/repeated inputs. Both benchmark binaries built. Full workspace tests and constructor cold-start timing were not run for this rejected candidate.

Six independent alternating baseline/candidate pairs for each workload; 24 fresh fixtures per process, one operation/sample, no warmup. Schema/preparation/setup, including constructor table initialization, excluded. Exact fixture/count/value/readback gates passed; thread/embedding/index work remains in the timed insert. No compilation/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -3.53% | 1/6 |
| compare/insert_bulk_1k/sqlite | -1.03% | 2/6 |
| compare/insert_bulk_10k/lin | -4.38% | 1/6 |
| compare/insert_bulk_10k/sqlite | 1.09% | 4/6 |
| compare/durable_insert_1k/lin | 1.20% | 4/6 |
| compare/durable_insert_1k/sqlite | 1.79% | 4/6 |
| compare/durable_insert_10k/lin | 1.87% | 5/6 |
| compare/durable_insert_10k/sqlite | 1.24% | 5/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Both native Lin workloads regressed in five of six pairs; native 10k SQLite control improved, making the native regression harder to explain as common host slowdown. Durable gains partly track positive SQLite controls and do not offset the native regression. Whole-insert measurements do not separately establish the CPU cost of range checks, table loads or cache effects.

Decision: reject this representation/lookup change. Production src/embed.rs restored byte-for-byte to baseline, retaining the separately measured bigram lookup. Raw sources, binary/source hashes, logs and observations saved. `git diff --check` passed after log trailing-blank normalization and restoration. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto measurements.
