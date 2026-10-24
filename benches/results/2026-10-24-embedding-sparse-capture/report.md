# Embedding sparse-coordinate capture: rejected

Baseline production source `0f2e2517c4fbd07b0dbc29f83a4705b533be5417`; no accepted engine change. Exact candidate embed/exec/persist sources retained; all three production files restored byte-for-byte. Original compare baseline reused only after checking every core source and executable hash. This differs from the old cached-count experiment: metadata is generated from normalized Scratch.touched entries, not by rescanning the output vector during classification.

## Candidate

Capture exact sorted (u32 index,u32 f32_bits) entries while default HashingEmbedder finalizes vectors in small durable batches (128..4095 rows). TLS metadata keyed by Arc allocation identity, with Weak preventing address reuse, is reused by sparse-size classification and legacy sparse emission. Missing metadata falls back to old scanning. No new codec, float approximation, sync/FTS/index/quota change. Native batches do not capture metadata but still execute the new disabled-path bookkeeping; large encoders also have lookup overhead, so they cannot be presumed timing-identical.

Bounds: at most 4095 entries, 2MiB charged coordinate storage and 8MiB logical vector backing (length*4+32 per cached vector). Hash-table/allocator overhead is additional; these are not whole-process RSS bounds. Weak references can defer freeing a vector allocation until cache reset even after its strong owners disappear. Cache clears at the next enabled batch, not immediately after commit. These lifetime/resource tradeoffs are not accepted into production.

## Correctness evidence

Two focused capture tests and full default workspace passed; release compare/crash/interleaved builds passed. Metadata sorted and exactly equal to original vector nonzero bits, optional/noncaptured/reset behavior and logical caps checked; manual special-bit fixture covers -0/NaN payload/Inf. Actual process-exit fixtures recovered 1000/3000 acknowledged rows with nine-field and every-vector-bit equality through a fresh read-only reader. One-commit WAL is byte-identical to the earlier frozen baseline fixture, with unchanged codec-1 schema and codec-3 data frame; three commits retain codec 3. This verifies actual replay, not physical power loss or every error/concurrency case.

## Conventional paired comparisons

Two independent six-pair alternating baseline/candidate series, native and Full durable 1k/10k, 24 fresh samples per case/process, one operation/no warmup. Exact readback outside timing; SQLite unchanged control. All raw observations retained, no builds/tests overlap timing, unrelated host work uncontrolled. Positive paired gain means faster.

| Case | Series 1 | Series 2 | Faster pairs S1/S2 |
|---|---:|---:|---:|
| insert_bulk_1k/lin | +0.147% | -2.301% | 3/6, 0/6 |
| insert_bulk_1k/sqlite | -1.494% | -2.981% | 2/6, 0/6 |
| insert_bulk_10k/lin | +10.979% | -4.143% | 3/6, 0/6 |
| insert_bulk_10k/sqlite | -0.074% | -2.139% | 3/6, 1/6 |
| durable_insert_1k/lin | +4.279% | -5.275% | 6/6, 2/6 |
| durable_insert_1k/sqlite | +2.344% | -9.406% | 5/6, 1/6 |
| durable_insert_10k/lin | +0.178% | -4.600% | 3/6, 2/6 |
| durable_insert_10k/sqlite | +1.014% | +0.393% | 4/6, 3/6 |

First-series native 10k and later durable controls have substantial timing shifts. Those values cannot isolate code causation and are retained rather than selected away.

## Same-campaign interleaved checks

Additional identical-baseline A/A followed by baseline/candidate A/B, same persistent-worker source and fresh Full fixtures. Six fresh worker pairs per size, 24 sample-by-sample alternating requests each; 1152 measured requests plus 48 preflights across both checks. Values/embedding bits checked each time, all owned directories removed and worker exits zero. A/A provenance separately identifies the frozen baseline source; current-source hashes refer to the uncommitted candidate for drift checks, not a claim the baseline executable contains it. Native Rust compare and interleaved helper binaries have distinct hashes retained.

| Check | Rows | Median gain | Median absolute pair difference | Faster pairs |
|---|---:|---:|---:|---:|
| aa | 1000 | +1.866% | 3.221% | 4/6 |
| aa | 10000 | +0.097% | 0.456% | 4/6 |
| ab | 1000 | +0.846% | 2.674% | 5/6 |
| ab | 10000 | -1.303% | 1.303% | 0/6 |

## Decision

Reject. Durable 1k conventional +4.28% then -5.28% is unqualified against control shifts. Interleaved 1k +0.85% is smaller than same-campaign A/A median absolute pair variation 3.22%; no distinguishable target gain. Interleaved 10k loses all six pairs (-1.30%) with much smaller A/A variation (0.46%). No second A/B repeat/RSS or server refresh warranted after this failed gate. Avoiding vector scans did not establish a useful complete-operation tradeoff after capture/sort/lookup overhead. No isolated attribution of every measured effect is claimed.

All evidence, including the rejected candidate and bit-exact WAL fixtures, is retained. Accepted source-specific read/native claims remain unchanged after restoration. Durable 1k SQLite gap, MSSQL/Kusto live qualification and full named-peer goal remain open.
