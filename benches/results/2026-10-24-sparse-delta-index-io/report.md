# Sparse delta indexes — encode/write/Full-sync feasibility

2026-10-24 baseline b2b3dff. Diagnostic-only proposed coordinate format; production unchanged. Current source-derived legacy/delta serial encoders use actual default 1k embeddings. The existing exact codec-3 full payload from ../2026-10-24-lz4-wal-cpu/payload-1000.bin is verified by its previously recorded SHA. Its legacy vector section is located uniquely at offset11946 and matches the current generated 238992-byte vector stream. Candidate replaces that section with152370 delta bytes; all other payload bytes remain unchanged. Explicitly experimental marker0 is used only in disposable files; this is not a valid new production reader/codec contract.

Two independent series ×six processes, each24 rotating rounds ×three variants: legacy A, byte-identical legacy B, delta. Total864 validated write/sync operations. Fresh owned files; current Apple empty-file1MiB best-effort reserve before timing for every variant (matching setup exclusion), with setup reservation times retained separately. Timer includes vector encoding, payload splice, checksum, frame-copy, current reserve check, write and copied production durable_sync. Remaining scalar encoding/row packing/classification, schema, model, file opening and teardown are outside timing. Same reusable buffers prepared before samples; this is standalone feasibility, not Db commit or cold lifecycle.

Full-sync helper retains Apple barrier followed by ordinary fallback on error; route is not instrumented in this gate, so no route/fallback-count or physical power-cut claim. All written bytes read back and compared; delta decoder re-expands vectors to exact original legacy bytes and verifies unchanged prefix/suffix. Temp files/directories removed only after validation. No production decoder, crash/replay or compatibility qualification.

| Series | Comparison | Baseline ms | Candidate ms | Median paired gain | Median absolute pair difference | Positive processes |
|---:|---|---:|---:|---:|---:|---:|
| 1 | AA | 0.757198 | 0.754042 | +0.733% | 0.871% | 4/6 |
| 1 | delta | 0.757198 | 0.749521 | +0.352% | 0.938% | 4/6 |
| 2 | AA | 0.743708 | 0.746333 | -0.508% | 0.766% | 2/6 |
| 2 | delta | 0.743708 | 0.737167 | +0.653% | 0.653% | 4/6 |

Legacy frames336981 bytes; delta frames250359 bytes (same336968/250346 payloads plus13-byte framing). Smaller full-frame bytes are real emitted bytes here, unlike the previous CPU report's projection. Payload splice is diagnostic reconstruction; a real versioned writer would need separate implementation and qualification.

## Decision: reject new-codec proposal on current evidence

Delta +0.352% /+0.653% is smaller than matched A/A median absolute differences0.871% /0.766%; only4/6 positive process pairs in either series. No calibrated encode/write/sync acceleration established. Earlier serial CPU penalty remains included. The substantial byte reduction does not justify format migration/reader complexity without a measurable end-to-end benefit. Do not implement a production codec or repeat unchanged candidate from this result. This is inconclusive performance evidence, not proof that byte reduction can never help other devices/workloads.

Raw samples, setup reserve costs, source/binary/payload hashes, gate/build/runner and all negative controls retained. No runner builds/tests overlap timing; unrelated host activity uncontrolled. No peak RSS, universal confidence interval, p99, engine latency or peer superiority claim. Unchanged production source verified against saved core hashes; no workspace rerun for diagnostic-only edits.

Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas goal remains open: durable SQLite gap and missing live MSSQL/Kusto comparisons persist.
