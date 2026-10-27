# Detailed default embedding phases inside real bulk insert

Unchanged production-main source hashes retained. Existing compare fixture, default embeddings/scalar index/FTS and checked full row readback. Three release macOS processes, two sizes,48 measured wall observations,66 retained diagnostic phase records including framework calls. Source restored and verified. No optimization or peer comparison.

| Inclusive stage | 1k us | 10k us |
|---|---:|---:|
| embedding_total | 303.375 | 3308.042 |
| feature_fill | 63.368 | 694.836 |
| normalize_output_reset | 88.541 | 920.262 |
| row_text | 43.245 | 421.514 |

Important limitation: extra per-row RAII timestamps/atomic increments materially increase diagnostic cost. Absolute totals must not be compared causally with the less-instrumented main-bulk-profile. Inclusive/nested medians do not add; remaining time includes lowercasing, per-row map access/insertion, batch preparation/allocation and instrumentation overhead. The normalization/output/reset phase itself combines several mechanisms and does not prove allocation alone dominates. Build_rows at10k has overlapping worker spans. No isolated-host, latency-tail, RSS, native/durable SQLite or eight-peer qualification.

Evidence changes next selection: feature hashing alone does not account for the full embedding stage. Do not repeat rejected ASCII trigram table, fused output or streamed-per-row experiments unchanged: their reports retain regressions. A potential distinct future hypothesis is batched reusable text with one scratch borrow while respecting custom embed_batch overrides, missing/existing embeddings and exact vector bits. It must first prove component and lifecycle equivalence; it is not implemented or accepted here. Raw logs, phase JSON, complete run artifacts and source/binary hashes retained.
