# Half-density sparse normalization: rejected

Baseline production 0f811a3 (production unchanged from retained classifier 4926d9c). Candidate changes only the sparse-normalization gate from touched*5 < dimension to touched*2 < dimension, retaining existing exact-quarter-weight reasoning and ascending-order fallback at large sums. No allocator, embedding feature weights, hash, vector format, WAL, sync or worker change.

Seven focused embedding tests pass, including new bit-exact comparisons at 153, 154, 200, 383, 384, 500 and 768 touched coordinates with small and large quarter weights. Scratch reset is checked. Benchmark build completes. Six alternating process pairs for native and Full durable, 24 fresh fixtures per case, one operation per sample, no warmup. No compilation or tests during timing. Binary hashes recorded. Positive paired gain means faster; wins are process-median wins.

| Case | Median paired gain | Wins | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | +0.230% | 4/6 | 0.8853 | 0.8868 |
| compare/insert_bulk_1k/sqlite | -0.611% | 3/6 | 0.8199 | 0.8193 |
| compare/insert_bulk_10k/lin | -0.047% | 3/6 | 8.9276 | 8.9689 |
| compare/insert_bulk_10k/sqlite | -0.067% | 3/6 | 10.9160 | 10.6941 |
| compare/durable_insert_1k/lin | +0.520% | 4/6 | 3.9668 | 3.7934 |
| compare/durable_insert_1k/sqlite | -0.116% | 3/6 | 2.1714 | 2.2686 |
| compare/durable_insert_10k/lin | +2.948% | 4/6 | 23.7019 | 23.0014 |
| compare/durable_insert_10k/sqlite | -4.945% | 2/6 | 28.2893 | 28.3303 |

Small native/durable gains (0.23% and 0.52%) do not close the SQLite gap, native 10k is flat, and durable 10k is inconsistent with a material opposite control shift. No repeat or full workspace run performed: insufficient evidence to retain the gate change. Production src/embed.rs restored byte-for-byte from before snapshot; candidate and tests preserved here only. Named-peer objective remains incomplete, including small SQLite insert gaps and missing MSSQL/Kusto configuration.
