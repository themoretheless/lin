# Sparse index/value packed entry — engine gate

2026-10-24. Baseline source: 2de2872 (production core ca9eec4). Candidate changes only the eight-byte sparse entry construction from two four-byte slice copies to an explicitly little-endian u64. Prior CPU-only gate: ../2026-10-24-sparse-branch-entry-cpu/report.md. No sync, frame, checksum, classifier, embedding, FTS, index, or peer setting changes.

Both compare binaries built fresh from the snapshots in this directory, with source/binary hashes and compiler versions in provenance.json. Candidate full default cargo test --workspace passed; exact-byte CPU fixtures already covered None/empty/positive and negative zero/NaN payload/infinity/tail/prefixed buffers. Source is restored to baseline after building, before measurement. Benchmark fixture validation runs outside timers. This is not physical power-cut qualification.

Engine method: two independent series × six alternating fresh process pairs, native and Full durable insertion at 1k/10k, 24 samples/case, one operation, no warmup, unchanged SQLite control. Setup/preparation/validation/drop excluded. No builds/tests intentionally overlap measurement; unrelated host activity uncontrolled. Raw runs, errors, outliers and rejected results are retained. Paired percentages are medians of pair changes, not ratios of aggregate medians. Native controls are useful noise observations; this candidate only modifies durable WAL serialization.

All 48 benchmark processes exited zero with complete samples. Candidate is **not accepted**: target 1k improvement did not repeat consistently above controls/noise. Production source remains byte-identical to baseline. Overall eight-peer objective remains open, including durable 1k SQLite and missing live MSSQL/Kusto qualification.

## Results

| Case | Engine | Series 1 paired gain /positive pairs | Series 2 paired gain /positive pairs |
|---|---|---:|---:|
| insert_bulk_1k | lin | -1.212% /2/6 | -2.521% /1/6 |
| insert_bulk_1k | sqlite | +0.342% /3/6 | -1.737% /2/6 |
| insert_bulk_10k | lin | -0.087% /3/6 | -3.508% /1/6 |
| insert_bulk_10k | sqlite | -0.674% /2/6 | +2.723% /4/6 |
| durable_insert_1k | lin | -0.003% /3/6 | +3.605% /4/6 |
| durable_insert_1k | sqlite | +0.388% /3/6 | +2.007% /5/6 |
| durable_insert_10k | lin | +0.648% /4/6 | -0.150% /3/6 |
| durable_insert_10k | sqlite | +2.237% /6/6 | -0.202% /2/6 |

Durable 1k candidate medians 1.945/1.963 ms versus SQLite 1.376/1.356 ms: gap remains. First target series is flat; second +3.6% overlaps the scale of observed control/native shifts. This does not prove a causal regression or acceleration. Small CPU gain (~10 us per 1k) did not establish an end-to-end benefit. No production patch, peer requalification, or physical power-cut claim. Candidate snapshot, both binaries hashes, successful workspace log and all outliers retained.
