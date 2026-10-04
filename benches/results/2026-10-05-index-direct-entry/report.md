# Direct index traversal and single-entry unique check: rejected

Baseline source 53a123b (production unchanged from retained 4926d9c). Candidate replaces slab insertion's allocated cloned-label list and repeated get_mut lookups with values_mut traversal filtered by collection. In insert_at_new, unique duplicate rejection moves from contains_key precheck to the Occupied entry branch before any reverse/forward mutation, avoiding double lookup for unique keys. Index order and error message preserved. Both changes measured together: no individual causal attribution.

45 lib unit tests pass, including inline/spilled unique keys and reverse updates. Release benchmark builds. Six alternating process pairs per native/Full durable mode, 24 fresh fixtures per case, one operation per sample, no warmup. Pinned binary hashes retained; no builds/tests overlap timing. Existing exact fixture validation completes. Positive paired gain means faster; wins count process-median pairs.

| Case | Median paired gain | Wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.171% | 2/6 |
| compare/insert_bulk_1k/sqlite | -1.296% | 3/6 |
| compare/insert_bulk_10k/lin | -4.179% | 3/6 |
| compare/insert_bulk_10k/sqlite | +0.682% | 4/6 |
| compare/durable_insert_1k/lin | -2.817% | 2/6 |
| compare/durable_insert_1k/sqlite | +2.353% | 4/6 |
| compare/durable_insert_10k/lin | +0.765% | 4/6 |
| compare/durable_insert_10k/sqlite | +4.146% | 5/6 |

No repeat or full workspace run: native 1k flat, durable 1k regresses and native 10k mixed/slower. Reject both changes rather than retaining a cleaner-looking implementation without measured gain. This workload does not isolate the unique-index branch benefit; no claim about that branch alone. Source store/index restored byte-for-byte from before snapshots. Candidate and tests/logs/raw observations preserved here only. Named-peer goal remains incomplete: native Rust SQLite 1k and durable SQLite 1k gaps, absent MSSQL/Kusto and remaining write coverage.
