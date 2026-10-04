# Buffered dictionary vector encoder: rejected

Baseline: 0b31508, production codec 3. Candidate starts from the prior rejected experimental codec 4/tag 8 source, but replaces its encoder with block-zero skipping and a single value-code lookup per nonzero entry. An inline SmallVec buffer of 64 (u32 index,u8 code) pairs feeds final serialization without another full dense-vector scan or repeated dictionary lookup. Larger entry sets spill to temporary heap storage. Dictionary overflow still re-encodes raw pairs. Float bits, dimension-dependent positions, None/empty distinction, CRC/fsync and codec rejection/reader semantics remain those of the prototype. No experimental codec was previously deployed.

Six independent alternating baseline/candidate pairs; 24 fresh one-operation samples per size/process, no warmup. Setup/schema/preparation excluded; normal synchronous durable insertion including encode/write/sync measured. Exact affected-count/value/readback checks passed. Builds/tests did not overlap timing.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -11.85% | 0/6 |
| compare/durable_insert_1k/sqlite | 1.19% | 4/6 |
| compare/durable_insert_10k/lin | -15.38% | 0/6 |
| compare/durable_insert_10k/sqlite | 0.73% | 4/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). Lin loses all six pairs at both sizes while SQLite controls are slightly faster/mixed. Decision: reject. These runs compare directly against production codec 3, not side-by-side against the earlier dictionary encoder. The smaller recorded regression than the earlier experiment is not a controlled measurement of improvement between encoder implementations. Whole-operation timing does not isolate dictionary lookup, buffering, memory traffic or fsync as a cause.

Eight focused WAL integrity tests passed, including legacy sparse/checksum cases and dictionary exact -0/NaN/Inf/presence/empty values, 65536/65537 width boundaries, raw overflow escape, truncation, malformed codes/tables/indices/budget. Benchmark builds passed. Full workspace suite was not rerun for this rejected buffer variant; the prior dictionary prototype's full suite is not claimed as validation of this changed encoder. Candidate source/test snapshots identify the provisional codec-number assertion (4) along with its other integration checks.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved hashes and raw samples retain the experiment. `git diff --check` passed after log trailing-blank normalization. No dictionary codec changes remain in production. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements.

A distinct next compression candidate may reduce coordinate width without value dictionaries or extra per-row buffers, retaining exact bits and dimension-based fallback. It still requires reader compatibility/safety tests and measured durable latency; smaller bytes alone are insufficient.
