# Current 100k chunked native insertion stage profile

Baseline production source `311e11f`, after rejection/restoration of memory marker lifetime candidate. Exact baseline and instrumented exec.rs, binary/source hashes, build exit status, raw phase logs and event JSON retained. Temporary Instant timers/eprintln compiled into a pinned release peer_bench; production source restored before running and verified unchanged after. No profiling code landed.

Three independent worker processes, each one request for 100k rows with 12 fresh samples; ten prepared 10k chunks in one memory transaction. Total 36 full insertions / 360 chunk events. Public default quotas, default embedding/FTS, unique id/uri and wing/ts indexes retained. The worker checks affected count, embeddings and every exact six-field value; Python independent fixture oracle validates all final result rows per process. Event counts and start offsets 0..90k are checked for every sample.

Phase scopes: build includes existing parallel row construction; checks includes default ID/hash fields, batch/existing identities and FKs; embedding separate; scalar indexes, FTS, HNSW, row maps separate. total_before_store covers from bulk entry through row maps, before final store extend/Handle output. HNSW inactive in this fixture (near-zero calls are not real ANN insertion throughput). Remainder includes reserves/edges/timer overhead; outer transaction/program/mark-written/destruction and final store append are outside the total span. Timers/logging perturb execution. No peer speedup or whole-engine gain claimed.

Aggregate **within each full sample first**, summing its ten disjoint chunk spans; then take median over 36 samples. Do not sum independent phase medians to derive a whole sample latency. Per-chunk distributions in phase-medians.json.

| Span | Median per-full-sample summed span, ms |
|---|---:|
| build_ns | 11.461 |
| checks_ns | 28.642 |
| embed_ns | 31.445 |
| index_ns | 12.978 |
| fts_ns | 14.350 |
| hnsw_ns | 0.001 |
| maps_ns | 6.483 |
| total_before_store_ns | 107.808 |

Checks/default fields and embedding dominate measured stages. Build is ~1.1–1.2ms per 10k chunk; checks ~2.6–3.1ms and embedding ~3.0–3.2ms. Existing identities/index/map maintenance grow modestly with collection size; the total profile does not isolate each individual check's cost.

Next hypothesis: inspect short unique-body content-hash cache overhead (lookup, body-key allocation, insertion and eviction) within the checks stage versus direct bit-identical content_hash_arc. cached_content_hash currently caches up to ~1023 values while this fixture traverses 100k unique short bodies. This is a source-based candidate, not yet a measured hash-cache bottleneck. Qualify repeated/long/Unicode bodies before adopting any length crossover; preserve exact hashes and existing data semantics. Embedding remains a separate target; rejected zero-marker attempts remain negative evidence.
