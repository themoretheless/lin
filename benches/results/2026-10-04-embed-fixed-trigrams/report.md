# Fixed-size trigram hash input: rejected

Baseline: 0041eef. Candidate passes [window[0], window[1], window[2]] into Scratch::bump rather than its three-byte slice, to expose constant length through the generic Hash input type. No additional table/heap allocation, model identity or wire-format changes. Array/slice hashing preserves the observed vector bits; sources and binary hashes identify the measured change.

Six alternating independent process pairs per series, 24 fresh fixtures per process, one operation/sample and no warmup. Fixture/schema/preparation excluded; normal insert work including embedding and durability sync remains timed. Exact fixture/count/value/readback checks passed. No builds/tests overlapped timing. Independent 1k repeat used the same pinned binaries without rebuilding.

Mixed-size series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 0.91% | 5/6 |
| compare/insert_bulk_1k/sqlite | -1.73% | 2/6 |
| compare/insert_bulk_10k/lin | -0.01% | 3/6 |
| compare/insert_bulk_10k/sqlite | 0.22% | 4/6 |
| compare/durable_insert_1k/lin | 1.97% | 4/6 |
| compare/durable_insert_1k/sqlite | -0.26% | 3/6 |
| compare/durable_insert_10k/lin | 0.40% | 4/6 |
| compare/durable_insert_10k/sqlite | 0.04% | 3/6 |

Independent native 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.23% | 2/6 |
| compare/insert_bulk_1k/sqlite | -0.26% | 2/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Initial small-native gain is weak and not replicated: the independent repeat changes Lin -0.23% and SQLite control -0.26%, both faster in only 2/6 pairs. Larger native is essentially unchanged. Durable indications are mixed and do not prove a generally useful improvement. No generated-code inspection was performed; these wall times do not establish whether compiler specialization removes checks or yields identical machine code.

Decision: reject the unconfirmed performance change. Production src/embed.rs restored byte-for-byte to baseline, retaining the separately validated ASCII whitespace optimization. Six focused embedding tests passed, including dense-reference f32 bits for all ASCII characters, Unicode whitespace, existing mixed/empty/repeated texts and dimensions 8/9/32/384/768/1024/1536. Both benchmark binaries built. Full workspace tests were not run for this rejected candidate. `git diff --check` passed after log trailing-blank normalization and source restoration.

The full nine-engine objective remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements. This is evidence against retaining this specialization, not a new speedup or peer matrix refresh.
