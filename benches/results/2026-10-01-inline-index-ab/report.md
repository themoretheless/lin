# Retained short index keys and search bounds

Index keys and search-bound temporaries use SmallVec with two inline parts. Larger composite keys spill to a heap buffer. The private key type preserves lexicographic ordering and numeric representation.

## Insertion key storage

Three initial process pairs won full 10k insertion by 6.5%, 11.1%, 6.7%. Builds between these runs are a possible disturbance. A second experiment built both binaries before measurements, then alternated order across six independent process pairs (12 fresh fixtures per case).

| Pair | Vec full, ms | Inline full, ms | Improvement |
|---|---:|---:|---:|
| 1 | 17.097 | 14.913 | 12.8% |
| 2 | 16.391 | 15.676 | 4.4% |
| 3 | 14.879 | 15.961 | -7.3% |
| 4 | 14.870 | 14.115 | 5.1% |
| 5 | 13.783 | 14.498 | -5.2% |
| 6 | 15.764 | 15.393 | 2.4% |

Median paired full-insert improvement: 3.4%; four of six pairs won. No-embedding improvement: 3.7%; unchanged no-scalar-index control: -3.1% (slower). Background load is uncontrolled; this is local evidence, not a universal causal percentage.

## Final search bounds

The final source also keeps equality prefixes and lower/upper search bounds inline. Both final and original Vec binaries were built before running six alternating independent process pairs, with 24 calibrated samples per case.

| Case | Vec process-median aggregate | Final process-median aggregate | Median paired improvement | Pairs won |
|---|---:|---:|---:|---:|
| filter_eq | 1.318 µs | 1.165 µs | 10.6% | 6/6 |
| filter_range | 2.244 µs | 2.147 µs | 4.3% | 5/6 |

## Final native ingestion

One independent process, 12 fresh-fixture samples per case. Affected count and row readback validators passed outside timing. DuckDB Appender includes flush inside timing.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.543 ms | 1.132 ms | 1.522 ms |
| 10k | 19.440 ms | 16.477 ms | 13.048 ms |

Lin still loses native ingestion comparisons. Native schema work differs: Lin computes embeddings and maintains FTS/indexes as documented in the contract. No claim of beating all peers is made. MSSQL/Kusto live comparisons remain missing.

## Validation

`cargo test --workspace --offline` passed on the final source. The new unit test checks short/spilled key ordering against Vec, unique conflicts, reverse-key replacement and deletion for arities 1, 2, 3, 5. Existing index, mutation, rollback, numeric and persistence tests passed. `cargo fmt --all -- --check` and `git diff --check` passed. Sources, hashes, raw runs, build logs and prebuilt replay script are included. The build-only filter deliberately selects no cases and can exit nonzero; build success is checked by the executable emitted by Cargo.
