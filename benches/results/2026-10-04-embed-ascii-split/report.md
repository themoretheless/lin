# ASCII whitespace splitting in hashing embedding: retained

Baseline: 2182950. Candidate uses split_ascii_whitespace for ASCII text without vertical tab; all other text keeps split_whitespace. The vertical-tab guard preserves the original Unicode whitespace semantics. Common token feature work is extracted into a private bump_token helper. All unigram/trigram/bigram ordering, weights, normalization, model identity, arbitrary dimensions and persisted f32 bits stay unchanged. No new table, public API, wire format or durability tradeoff.

Six alternating independent baseline/candidate pairs per series; 24 fresh fixtures/process, one operation/sample, no warmup. Preparation/schema/setup excluded; embedding/thread/index/durable flush work remains timed. Exact fixture/count/value/readback gates passed. Builds/tests did not overlap timing. Repeat used the same pinned binaries without rebuilding.

Mixed-size insertion series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 13.83% | 6/6 |
| compare/insert_bulk_1k/sqlite | 3.86% | 4/6 |
| compare/insert_bulk_10k/lin | 5.30% | 6/6 |
| compare/insert_bulk_10k/sqlite | 3.23% | 4/6 |
| compare/durable_insert_1k/lin | 3.58% | 5/6 |
| compare/durable_insert_1k/sqlite | 7.34% | 5/6 |
| compare/durable_insert_10k/lin | 1.10% | 3/6 |
| compare/durable_insert_10k/sqlite | 5.25% | 5/6 |

Independent native 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 6.16% | 6/6 |
| compare/insert_bulk_1k/sqlite | -0.77% | 2/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Mixed-size native gains are partly confounded by positive SQLite controls. The independent small-native repeat favors Lin in all six pairs (+6.16%) while SQLite control is approximately flat/slower (-0.77%), supporting a repeatable improvement on that workload. Durable gains are mixed and SQLite controls improved more; no durable acceleration is established by this evidence. Larger-native results also include positive control movement and are not a universal causal claim. Host load/cache/frequency remain uncontrolled, and absolute medians differed between series; no pooling of the two series was performed.

Candidate native 1k still loses to SQLite: mixed-size aggregate medians 1.250948 vs 1.216781 ms (0/6 peer wins), repeat 1.155813 vs 1.107542 ms (1/6 wins). Native 10k favors Lin in all six mixed-size pairs. Durable 1k/10k remain slower than SQLite. The full nine-engine goal remains incomplete, including missing MSSQL/Kusto endpoint proof.

Validation: six focused embedding tests passed. The original dense-reference bit test was expanded across all 128 ASCII characters between tokens (including VT, FF, CR/LF, tabs, NUL and mixed case), Unicode whitespace U+0085/U+00A0/U+1680/U+2000/U+200A/U+2028/U+2029/U+202F/U+205F/U+3000, plus existing ASCII/Unicode/empty/repeated inputs, at dimensions 8/9/32/384/768/1024/1536. Full `cargo test --offline --workspace` passed. Both benchmark builds passed. `git diff --check` passed. final-embed.rs adds only an explanatory comment after testing/measurement; benchmark candidate source and source/binary hashes are saved separately.

Decision: retain the repeated small-native improvement with exact compatibility. These measurements do not prove Unicode-path performance, neural-backend performance, per-engine CPU superiority or physical disk portability.
