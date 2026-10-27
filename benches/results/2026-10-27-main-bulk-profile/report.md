# Production-main native bulk insertion phase profile

Source: main047 production Rust sources, identified by before_sha256 in provenance.json; enclosing evidence branch commit is not the production-code baseline. Instrumented existing benches/compare.rs, retaining exact setup_lin_insert/fill_lin and checked validation, default embeddings, scalar index and FTS. No candidate optimization. Three fresh macOS release processes, sizes1k/10k,8 wall samples per case =48 wall observations. Probe retains all22 phase records per process,66 total, including framework calibration/validation calls rather than silently discarding them. Aggregation: median per process then median of three processes.

| Inclusive stage | 1k us | 10k us |
|---|---:|---:|
| run_prepared | 733.500 | 7637.959 |
| insert_bulk | 732.875 | 7634.167 |
| build_rows | 144.167 | 2084.792 |
| hash_and_validate | 88.292 | 987.542 |
| embedding | 254.250 | 2789.750 |
| maintenance | 252.625 | 2651.208 |
| scalar_index | 87.708 | 898.334 |
| fts | 133.583 | 1331.084 |
| hnsw | 0.041 | 0.042 |
| row_maps | 32.416 | 431.667 |

Diagnostic only. RAII/atomic instrumentation changes cost; nested stage medians do not sum. build_rows calls can overlap on two threads at10k, so their accumulated elapsed time is not serial wall time and must not be subtracted from total. Host had other work; no isolated CPU or peer qualification. Probe output occurs at process exit, not inside each timed operation. Exact existing six-field/default-vector readback runs after measured calls; phase logs, parsed records, raw benchmark artifacts and hashes retained.

Dominant observed phases: embedding and maintenance (within maintenance, FTS and scalar indexes). Next investigation should inspect embedding/text construction and FTS token processing with exact vector/posting equivalence; do not disable either to obtain a win. The earlier autocommit payload-copy optimization concerns UPDATE/DELETE and does not explain bulk insertion: large inserts already move built rows into Store.

Premeasurement correction: first built probe unnecessarily cloned Prepared outside the timer; no measurement ran with that binary. It was removed and rebuilt; original build/log/provenance/compare source retained under premeasurement-cloned-prepared-*. Final source restoration verified against every before_sha256. Full broader read/native/durable/eight-peer qualification remains open.
