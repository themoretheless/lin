# Sparse-only fused embedding output — engine gate

2026-10-24 baseline 2e1f81b. Existing sparse branch only: normalize directly while initializing output Arc, leave scratch unnormalized until touched cleanup. Dense finish retains original l2_normalize plus Arc copy. No new crossover, text-specific dispatch, format/model/feature/hash/order/index/FTS/sync change. CPU gate in ../2026-10-24-embedding-sparse-fused-output-cpu/report.md; this engine candidate is formatted before building, same logic.

Fresh baseline/candidate builds from saved embed.rs snapshots. Exact source/binary hashes and compiler versions in provenance.json; full default cargo test --workspace passed. Source restored before measurement and checked again afterward. Every output slot is written in an equal-length loop before assume_init. Existing independent dense-reference/normalization tests plus source-derived finite bit-exact CPU checks are correctness evidence, not exhaustive arbitrary scratch-state proof or physical power-cut qualification.

Two independent series ×six alternating fresh process pairs, native/durable 1k/10k, 24 samples/case/process, no warmup, one operation. Setup/schema/preparation/exact fixture validation/drop excluded; unchanged SQLite controls. No runner builds/tests overlap measurement; unrelated host activity uncontrolled. Paired gain is median of process-pair percentages, not aggregate ratio. All outliers retained. Native/durable lifecycles remain separate; no peer coverage is inferred from CPU results.

## Results

All 48 processes completed with 24 samples/case and successful fixture validation.

| Case | Engine | Series 1 paired gain /positive pairs | Series 2 paired gain /positive pairs |
|---|---|---:|---:|
| insert_bulk_1k | lin | -0.482% /3/6 | -0.035% /3/6 |
| insert_bulk_1k | sqlite | -2.191% /1/6 | -0.791% /1/6 |
| insert_bulk_10k | lin | -2.010% /2/6 | -0.865% /1/6 |
| insert_bulk_10k | sqlite | -0.281% /2/6 | -1.604% /1/6 |
| durable_insert_1k | lin | -0.494% /2/6 | -2.934% /2/6 |
| durable_insert_1k | sqlite | -0.432% /3/6 | -1.689% /2/6 |
| durable_insert_10k | lin | -0.392% /2/6 | -0.553% /1/6 |
| durable_insert_10k | sqlite | -1.316% /2/6 | +0.430% /3/6 |

## Decision: reject

No repeatable full-engine acceleration. Target durable 1k -0.494% /-2.934%, both 2/6 positive pairs; native 1k essentially flat; 10k negative in both modes/series. Controls also shift, so no isolated causal slowdown mechanism is asserted. Small CPU component benefit did not justify a production unsafe initialization path. No unchanged-candidate additional repetitions or peer refresh needed for rejection.

Candidate durable 1k medians 2.109/2.599 ms still lose SQLite 1.331/1.410 ms. Existing accepted production read/native matrix is not invalidated by this rejected candidate and does not qualify it. Production embed.rs matches baseline bytes, verified using all core source hashes. No source patch accepted. Both snapshots, successful workspace log, raw observations and CPU gate retained.

No peak RSS, new process-exit replay, physical power-cut or optional-platform qualification performed for this rejected source candidate. Full eight-peer objective remains open, including durable SQLite gap and absent live MSSQL/Kusto comparisons.
