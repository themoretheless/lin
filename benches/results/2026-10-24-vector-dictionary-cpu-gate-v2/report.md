# Vector dictionary CPU gate v2

Production source `4ee40c7ac386e01eb7587c3ef604731b73588f12`; core sources unchanged and hashes verified after all runs. Diagnostic Rust source gate.rs, exact current sparse reference, build/status logs, pinned binary hash, six process JSONL outputs retained. Temporary Cargo example removed immediately after build. No production codec, reader, WAL file or runtime setting changed. No workspace rerun: this is a diagnostic executable, not a shipped engine patch.

## Scope and method

Real default 768-dimension embeddings created by Db::empty insertion using the Rust compare fixture (title/body/id/uri/wing/layer, wing+ts index, fixed timestamps), then Arc-owned immutable vectors retained for CPU trials. Setup and model work excluded. Six independent release processes; twelve alternating old/new function rounds per case/process, buffer capacities reserved outside timing and buffers reused/cleared. First-touch sample remains included; no outliers removed. Per-process medians aggregated before paired percent gains. No competing tests/builds/benchmarks initiated by this runner overlap timing; host activity otherwise uncontrolled.

None/Some(empty), positive/negative zero, distinct quiet/signalling NaN bits, +/-Inf, partial blocks, dimensions 65535/65536/65537 and high-cardinality escape are bit-roundtripped, including forced dictionary mode and an existing output prefix. Decoder here is trusted-fixture/assert-based **not production untrusted-input code**. No fuzz/crash/reader compatibility safety claim.

Row proposal: old dimension/count headers, <=16 exact-bit dictionary values, u16 indexes at dimensions<=65536 (otherwise u32), u8 value ordinals, raw u32-index/u32-bits escape. Encoded dictionary selected only when beneficial; baseline code is mechanically copied from current persist.rs. This is component CPU/byte evidence, not full-frame size, sync speed, DB throughput, operation p99 or a peer win.

Reference is the existing **serial sparse append** routine, excluding sparse classification, decoded-budget checks, fields/framing/checksum/sync. 10k here is serial kernel evidence, not current four-way production wall time. v1 uses variable-size 3/5-byte entry copies; v2 constant-size writes; v3 also probes the first four zero-initialized dictionary slots before searching the tail. The first three candidates have CPU penalties even though default vector bytes shrink.

| Case | Reference ms | Candidate ms | Paired gain | Faster processes | Reference bytes | Candidate bytes | Dictionary selected |
|---|---:|---:|---:|---:|---:|---:|---|
| default_1k | 0.231789 | 0.304962 | -30.752% | 0/6 | 238992 | 112542 | True |
| default_10k | 2.901066 | 3.859917 | -33.094% | 0/6 | 2539664 | 1180574 | True |
| high_cardinality_sparse | 0.140217 | 0.338336 | -141.021% | 0/6 | 520000 | 520000 | False |
| mixed_bit_boundaries | 0.025478 | 0.034581 | -35.464% | 0/6 | 764 | 750 | True |

Decision: do not adopt this serial encoder alone. Every process loses CPU time on default vectors; high-cardinality fallback also wastes work before retaining legacy bytes. Preserve negative results. The v4 eligibility/whole-choice prototype is a separate experiment; it does not erase these costs.
