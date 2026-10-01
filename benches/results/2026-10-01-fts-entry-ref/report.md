# Borrowed FTS hash entry experiment — rejected

Candidate replaces only the private postings map with hashbrown 0.17.1 and FxBuildHasher, and uses entry_ref to avoid a second hash-table lookup for newly seen tokens. Adds/dels remain the existing maps. Tokenization, sorted posting lists, query logic and durable format unchanged. Dependency and candidate source retained as artifacts, removed from production on rejection.

Full offline workspace tests pass, including FTS append/recycled-tail, pending-order, fold/codec and durable replay coverage. Formatting and diff checks pass. Pinned optimized binaries; builds/tests did not overlap benchmarks. Alternating process order, uncontrolled host load, fixed per-process case order.

## Native inserts

Six process pairs, 24 fresh fixtures per case, one operation each; full exact rows checked outside timer. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.029646 | 1.047667 | 10.969604 | 11.084376 |
| 2 | 1.011625 | 1.027583 | 10.634208 | 10.883000 |
| 3 | 1.003667 | 0.995541 | 10.770478 | 10.560645 |
| 4 | 1.004812 | 1.005271 | 10.355000 | 10.376397 |
| 5 | 1.060417 | 1.000959 | 10.388562 | 10.457438 |
| 6 | 1.027125 | 1.006917 | 10.735291 | 10.431625 |

1k faster 3/6, median paired decrease 0.38%; 10k faster 2/6, median decrease -0.43%. No stable insert gain.

## FTS reads and 10k single-row writes

Three process pairs, 16 samples each. Reads: batches capped at 64 iterations; writes: one operation/fresh fixture with readback checks. Microseconds per operation.

| Case | Baseline medians | Candidate medians |
|---|---|---|
| compare/fts_lex_selective/lin | 0.730, 0.725, 0.693 | 0.800, 0.755, 0.717 |
| compare/fts_lex_common/lin | 180.229, 176.042, 180.167 | 182.740, 177.724, 182.216 |
| compare/fts_lex_miss/lin | 0.276, 0.272, 0.271 | 0.284, 0.273, 0.272 |
| compare/update_1row_10k/lin | 8.562, 8.938, 8.459 | 8.979, 8.687, 9.250 |
| compare/update_1row_10k/sqlite | 9.854, 10.229, 8.709 | 10.771, 8.834, 11.625 |
| compare/delete_1row_10k/lin | 14.938, 14.438, 15.188 | 16.208, 15.229, 14.771 |
| compare/delete_1row_10k/sqlite | 10.625, 8.584, 10.021 | 8.812, 8.229, 10.542 |

Every measured FTS read case loses 3/3 pairs. Selective read median paired decrease -4.13% (slower). Update/delete each win only 1/3 pairs; controls also vary. Candidate rejected and original FTS map and Cargo files restored. Previous WAL optimizations retained. No all-eight-peer superiority established.
