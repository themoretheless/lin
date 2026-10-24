# Vector dictionary CPU gate v4

Production source `4ee40c7ac386e01eb7587c3ef604731b73588f12`; core sources unchanged and hashes verified after all runs. Diagnostic Rust source gate.rs, exact current sparse reference, build/status logs, pinned binary hash, six process JSONL outputs retained. Temporary Cargo example removed immediately after build. No production codec, reader, WAL file or runtime setting changed. No workspace rerun: this is a diagnostic executable, not a shipped engine patch.

## Scope and method

Real default 768-dimension embeddings created by Db::empty insertion using the Rust compare fixture (title/body/id/uri/wing/layer, wing+ts index, fixed timestamps), then Arc-owned immutable vectors retained for CPU trials. Setup and model work excluded. Six independent release processes; twelve alternating old/new function rounds per case/process, buffer capacities reserved outside timing and buffers reused/cleared. First-touch sample remains included; no outliers removed. Per-process medians aggregated before paired percent gains. No competing tests/builds/benchmarks initiated by this runner overlap timing; host activity otherwise uncontrolled.

None/Some(empty), positive/negative zero, distinct quiet/signalling NaN bits, +/-Inf, partial blocks, dimensions 65535/65536/65537 and high-cardinality escape are bit-roundtripped, including forced dictionary mode and an existing output prefix. Decoder here is trusted-fixture/assert-based **not production untrusted-input code**. No fuzz/crash/reader compatibility safety claim.

Row proposal: old dimension/count headers, <=16 exact-bit dictionary values, u16 indexes at dimensions<=65536 (otherwise u32), u8 value ordinals, raw u32-index/u32-bits escape. Encoded dictionary selected only when beneficial; baseline code is mechanically copied from current persist.rs. This is component CPU/byte evidence, not full-frame size, sync speed, DB throughput, operation p99 or a peer win.

Reference is current **classification plus chosen sparse/dense emission**, including existing four-way classifier/encoder at 10k (copied helpers and dense loop). Candidate only considers dictionary at 128..4095 rows with 64KiB..8MiB dense metadata size, samples up to four rows for <=16-value eligibility and requires >=20% vector-byte savings. It encodes once, choosing from computed byte costs, avoiding a separate full-column sparse-classification pass on the eligible 1k path. High-cardinality and 10k controls use the existing path. JSON retains legacy sparse_ns/sparse_bytes field names, but v4 times the whole vector representation choice.

Metadata/probe overhead is included. Outer column/frame metadata, decoded-budget enforcement, scalar columns, record checksums/write/sync and actual production reader are excluded; these gates remain. Do not subtract earlier profiler medians to infer this gain: both compared choices execute on the same frozen input in the same process here.

| Case | Reference ms | Candidate ms | Paired gain | Faster processes | Reference bytes | Candidate bytes | Dictionary selected |
|---|---:|---:|---:|---:|---:|---:|---|
| default_1k | 0.321429 | 0.285384 | 11.620% | 6/6 | 238992 | 112542 | True |
| default_10k | 1.097540 | 1.096177 | 0.127% | 4/6 | 2539664 | 2539664 | False |
| high_cardinality_sparse | 0.224494 | 0.224611 | -0.164% | 2/6 | 520000 | 520000 | False |
| mixed_bit_boundaries | 0.025789 | 0.025817 | 0.081% | 4/6 | 764 | 764 | False |

Decision: CPU/byte feasibility passes for the 1k whole-choice prototype (6/6 process gains). Default 1k vector bytes 238,992 ->112,542 (-52.91%). Historical/current full WAL payload 336,968 would project to 210,518 bytes if other fields/framing stayed unchanged (~37.5% reduction); this is arithmetic, not an actually written frame or latency result. Default 10k intentionally retains the existing parallel representation, not the slower serial dictionary kernel.

Next required milestone: versioned production writer/strict bounded decoder, raw/dense/mixed fallbacks, all old-codec golden inputs, old pinned reader rejecting new complete frames without truncation, malformed/checksum/64MiB-budget tests, durable reopen/checkpoint/replay/crash and full workspace. Then two independent six-pair end-to-end native/durable SQLite-controlled series plus resource controls. Do not enable new writes merely because this CPU gate passes. See ../2026-10-24-current-wal-phase-profile/dictionary-codec-plan.md. Durable 1k gap and MSSQL/Kusto remain unqualified; complete named-peer objective stays open.
