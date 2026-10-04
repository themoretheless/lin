# Reusing sparse-size estimate for WAL buffer reserve: rejected after repeat

Baseline source 5001867 (production identical to retained 4926d9c). Candidate adds an output size to the existing sparse classifier, reuses its exact computed byte count in the selected sparse column to reserve buffer capacity, and preserves the Some(empty) early return with a zero size hint. No duplicate size scan, codec/schema/order/hash/float bits or Full-sync change. Size hint is ignored above MAX_RECORD to avoid unbounded reservation for rejected records. Large classifier remains parallel; sparse encoder remains four-way.

Ten existing focused persist unit tests pass, covering serialized reference bytes, worker fallback, sparse decoded budget and recovery. Benchmark build succeeds. Each round uses six alternating process pairs per mode, 24 fresh fixtures per case, one operation per sample, no warmup. Same pinned binary pair in both rounds; hashes retained. No tests or builds overlap timing. Positive gain is faster, wins count process medians.

| Case | First paired gain / wins | Repeat paired gain / wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | +2.414% / 4/6 | -1.987% / 3/6 |
| compare/insert_bulk_1k/sqlite | +4.453% / 5/6 | -13.053% / 1/6 |
| compare/insert_bulk_10k/lin | +2.344% / 4/6 | -13.099% / 2/6 |
| compare/insert_bulk_10k/sqlite | +4.536% / 4/6 | -8.019% / 2/6 |
| compare/durable_insert_1k/lin | +2.687% / 3/6 | -3.799% / 2/6 |
| compare/durable_insert_1k/sqlite | +4.353% / 3/6 | +1.892% / 4/6 |
| compare/durable_insert_10k/lin | +20.337% / 5/6 | +3.165% / 3/6 |
| compare/durable_insert_10k/sqlite | +5.402% / 4/6 | +9.426% / 4/6 |

The initial durable 10k gain does not reproduce: repeat wins only 3/6, control improves more, and durable 1k regresses. Native cases also shift despite not using the changed WAL reserve path; do not attribute all variation to the candidate. Reject rather than retaining a first-round gain. No full workspace run performed for this rejected change. Production src/persist.rs restored byte-for-byte; candidate implementation preserved here only. The all-named-peer goal remains incomplete, including SQLite small insertion gaps and missing MSSQL/Kusto endpoints.
