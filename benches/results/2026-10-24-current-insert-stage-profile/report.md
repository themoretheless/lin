# Current insertion-stage diagnostic

Source `488a74ccf6bc340f60408b6f489b80bc3b8be9b4`. Temporary timers in insert_bulk only; pinned release compare executable built then production exec.rs restored byte-for-byte before measurements. No production behavior change or peer speedup claim.

Three independent processes for each native and Full durable mode, 24 fresh samples per 1k/10k workload plus preflight. All six run.json complete, runner exits zero and existing exact fixture validation passed. Timer/eprintln overhead and uncontrolled host load perturb this diagnostic. No build/test work initiated by this runner overlaps measurement.

| Phase | Native 1k ms | Native 10k ms | Durable 1k ms | Durable 10k ms |
|---|---:|---:|---:|---:|
| build_ns | 0.140833 | 1.106209 | 0.189708 | 1.098750 |
| checks_ns | 0.089375 | 1.009500 | 0.134125 | 1.003209 |
| embed_ns | 0.252375 | 2.917833 | 0.391125 | 2.859541 |
| index_ns | 0.086167 | 0.903500 | 0.123625 | 0.915166 |
| fts_ns | 0.131541 | 1.326125 | 0.177041 | 1.327250 |
| hnsw_ns | 0.000041 | 0.000041 | 0.000042 | 0.000041 |
| maps_ns | 0.037500 | 0.495458 | 0.049500 | 0.489000 |

These are partial phase spans and medians of process medians. They cannot be summed into exact full operation latency or subtracted from another experiment to derive a speedup. Build includes row construction; checks includes hashes/defaults/identity/FKs; embedding includes text preparation and output attachment; index/FTS/HNSW/maps are slab maintenance calls. Outer transaction/setup/reserve/edges/final store append, WAL packing/encode/write/sync and teardown are outside these spans. HNSW inactive here (about timer overhead), not evidence of ANN insertion speed. Durable and native phase differences do not establish a causal persistence overhead on each CPU stage.

## Consequences

Embedding remains the largest measured CPU phase; row construction and FTS are next for small durable batches. Existing WAL stage diagnostics remain complementary, source-specific evidence. After flat/rejected sparse-encoder variants, another block-width/capacity tweak is not supported. The previously rejected scoped batch embedding prototypes must also not be retried unchanged: see ../2026-10-04-scoped-batch-embed/report.md. Further work should isolate text preparation/feature extraction/output attachment within embedding or tokenization/posting maintenance within FTS before selecting a production candidate. No candidate acceptance inferred from this profile.

Current full peer goal remains incomplete: durable 1k SQLite gap and unavailable MSSQL/Kusto persist. Current six-peer read and five-peer native matrices are separate evidence, not completion of all contracts.
