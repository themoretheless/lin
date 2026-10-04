# Narrow sparse WAL coordinates: rejected as unconfirmed

Baseline: 1a2dc09. Candidate adds internal codec 5/tag 9 within the checksummed LIN\x06 envelope, with old codecs 1/2/3 still readable. Sparse columns >=256 rows encode coordinates as u16 for dimension <=65536, otherwise u32. Float bits, ordering, None/Some(empty), CRC, fsync and decoded sparse vector budget unchanged. No value dictionary or additional coordinate buffer. Old binaries reject codec 5; no such format was previously deployed.

Six alternating independent baseline/candidate process pairs per series, 24 fresh fixtures per case/process, one operation/sample and no warmup. Setup/schema/preparation excluded; normal synchronous durable operation remains timed. Exact affected-count/value/readback checks passed. No builds/tests overlapped timing. Independent repeat used the same pinned binaries without rebuilding.

Mixed-size series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | 8.13% | 5/6 |
| compare/durable_insert_1k/sqlite | -0.09% | 3/6 |
| compare/durable_insert_10k/lin | 0.94% | 4/6 |
| compare/durable_insert_10k/sqlite | 0.06% | 4/6 |

Independent 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | 1.28% | 4/6 |
| compare/durable_insert_1k/sqlite | 2.25% | 5/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Initial small-batch gain looked promising with a flat SQLite control. Independent 1k repeat does not reproduce a comparably strong isolated gain: Lin +1.28% in 4/6 pairs, SQLite +2.25% in 5/6. Large-batch indication is weak/mixed. These data do not establish a generally useful durable latency improvement and do not close the SQLite gap. Decision: reject the unconfirmed format tradeoff; smaller coordinates alone are not acceptance proof for the requested performance goal.

Narrow entry bytes decrease from 8 to 6 (coordinate plus exact f32 bits), a 25% per-entry reduction. Per-vector headers/other columns/frame bytes remain, and full WAL byte savings were not independently measured here. Values above the dimension boundary keep eight-byte entries.

Validation: eight focused WAL integrity tests passed. New cases cover bit-exact -0/NaN/Inf, all-zero/empty/missing vectors, last-coordinate width boundaries 65536/65537, all frame truncation prefixes, checksum flips, old decoder rejecting the new tag, trailing payload, decoded dimension budget, oversized counts and invalid/duplicate indices. Full `cargo test --offline --workspace` passed after the integration codec-number assertion was updated to 5; its one-frame/reopen/replay/value checks remained. Both benchmark binaries built. No broad fuzz campaign was run.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved source/test copies and hashes identify the complete candidate, including the provisional codec-number assertion. No new format remains in production. `git diff --check` passed after log trailing-blank normalization. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements.
