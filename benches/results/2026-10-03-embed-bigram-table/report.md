# Default embedding bigram lookup: retained

Baseline: 73e2f18. Replace repeated FxHasher slice hashing and modulo for whole-string byte bigrams with a shared 65,536-entry u16 table for dimension 768. Other dimensions retain the original path. The table is computed using the active platform's FxHasher, not baked hash constants. Feature order, weights, normalization, model identity and persisted vector bits remain unchanged.

Memory cost: 128 KiB of process-lifetime table payload plus allocation/OnceLock metadata. Construction initializes the table once, before embedding. Twelve independent release processes measured the actual first HashingEmbedder constructor: median 72,583.5 ns (about 73 us), including ordinary construction. Subsequent constructors were 17–24 ns in the first observed runs (see all raw values). No baseline cold-start comparison was measured; this is an explicit new one-time cost, not a cold-start improvement. Diagnostic constructor source and binary hash are saved; the temporary example was removed.

Six alternating baseline/candidate process pairs for each workload. Native and durable insert cases: 24 fresh fixtures per process, one operation/sample, no warmup. Setup includes constructing the database and therefore excludes the new initialization cost from measured insert time, as well as existing preparation/schema costs. Fixture validation checks affected rows and values outside timing. Compilation and tests did not overlap timed runs. SQLite controls are included below.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 4.87% | 6/6 | 0.977062 | 0.911980 |
| compare/insert_bulk_1k/sqlite | 3.12% | 4/6 | 0.835094 | 0.809031 |
| compare/insert_bulk_10k/lin | 2.48% | 5/6 | 10.388791 | 10.169438 |
| compare/insert_bulk_10k/sqlite | -0.36% | 2/6 | 10.708719 | 10.727854 |
| compare/durable_insert_1k/lin | 0.27% | 4/6 | 2.611375 | 2.592188 |
| compare/durable_insert_1k/sqlite | 5.75% | 5/6 | 1.604521 | 1.539021 |
| compare/durable_insert_10k/lin | 3.28% | 6/6 | 20.463980 | 19.804865 |
| compare/durable_insert_10k/sqlite | 0.98% | 4/6 | 14.114812 | 14.017531 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i), not a ratio of separate aggregate medians. Native 1k gains are partly confounded by SQLite control movement; durable 1k shows little improvement. Native 10k gains and durable 10k gains are more consistent than their controls, with durable 10k faster in all six pairs. This does not establish a universal improvement across machines/text distributions.

Warm hybrid-common read: six alternating pairs, 32 samples, 50 ms warmup, target 5 ms/sample, at most 100 iterations. Median paired improvement 0.09%, faster 4/6: effectively unchanged in this evidence.

Validation: all 65,536 byte pairs match the original slice hash/modulo; reference embedding tests match exact f32 bits across dimensions 8/9/32/384/768/1024/1536 and ASCII/Unicode/empty/repeated texts. All six focused embedding tests passed. Full `cargo test --offline --workspace` passed. Benchmark builds and diagnostic constructor release build passed. `git diff --check` passed. No wire-format or public signature change.

Decision: retain for the measured large-insert gains with the explicit memory and cold-start tradeoff. SQLite still wins native 1k and durable 1k/10k; Lin wins native 10k in these process medians. MSSQL and Kusto remain unmeasured without endpoints; the full nine-engine goal is not achieved.
