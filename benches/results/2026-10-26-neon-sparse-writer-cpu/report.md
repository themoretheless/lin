# NEON sparse writer — CPU feasibility

2026-10-26, frozen baseline f8fb0db. No production edits for this CPU experiment. The baseline helper is copied byte-for-byte from current append_sparse_rows_serial; reference and source hashes retained. This is a new NEON presence-mask implementation, distinct from the rejected scalar block-mask writer (2026-10-03) and classifier-only NEON count (2026-10-24). Neither earlier experiment is retroactively accepted.

Eight raw-bit lanes are loaded as two four-lane NEON vectors. All-zero blocks return immediately; integer compares and lane weights form a presence mask, then trailing-zero iteration emits index/value entries in original ascending order. Negative zero and NaN payload bits remain present. Partial chunks use scalar masks. Dim/count/header/error representation remains unchanged.

## Contract

Default real HashingEmbedder vectors from public Db fixtures,1k/10k; also zero vectors and a dense guard. Each timed call allocates its output buffer and serializes the vector column; destruction is outside timing. No classifier, scalar columns, parallel writer composition, frame, checksum, I/O, sync, transaction, embedding generation or index/FTS maintenance is timed.10k is deliberately the serial helper component, not the existing production four-way dispatch.

Serial A, identical serial B and NEON C rotate within24 rounds;2×6 fresh processes.3456 timed operations, all exact bytes compared against the original serial helper after timing. Finite raw-bit oracle covers lengths0..1024, four starting-pattern offsets, None,Some(empty),zeros,-0,NaN/Inf payloads and deterministic raw bits. Direct eight-value mask calls also test actual unaligned subslices at offsets0..3. This is finite qualification, not formal universal coverage. Output Arc slice construction itself copies; only the direct mask probes establish those unaligned load checks.

No own builds/tests overlap timing; source/binary hashes checked before/after, all12 processes exit0, every time positive/finite. Host load samples retained, not exclusive-host calibration.

| Case | Series1 gain /positive processes | Series2 gain /positive processes | A/A absolute difference series1 /2 |
|---|---:|---:|---:|
| default1k | +50.026% /6/6 | +50.955% /6/6 | 0.100% /0.142% |
| default10k serial | +35.537% /6/6 | +35.225% /6/6 | 1.466% /1.045% |
| zero1k | +27.402% /6/6 | +28.705% /6/6 | 0.042% /0.041% |
| dense1k guard | −2.005% /2/6 | −1.165% /0/6 | 1.001% /0.848% |

Dense guard loses; retained. Its all-nonzero768-dimension vectors would select the existing dense codec rather than this sparse helper in production. These calls probe the sparse writer outside its selected-codec dispatch and do not establish a dense-codec regression. Near-classifier crossover was subsequently measured in the separate subgate below.

## Decision

Proceed to full-engine qualification; **do not accept from CPU results**. The profile motivates the writer span, but a ~117µs component saving is not automatically a commit-latency gain. Keep fallback/oracle correctness and require two conventional native/Full durable series with SQLite controls, interleaved A/A/B, no regression of the accepted large-batch policy, replay/frame equivalence and resource observations before retention. All-eight-peer goal remains open.

## Density crossover subgate

Separate frozen diagnostic binary, same CPU helper implementations:1000 rows ×768 dimensions,383/192 nonzero values per vector, both spread and clustered positions. These sizes are below the existing sparse/dense crossover (383 entries:3072 sparse bytes versus3076 dense bytes per row). Two fresh six-process series,3456 operations, rotated A/A/C, exact bytes and the same finite raw-bit oracle. Library/source and gate hashes retained under crossover/. No production source mutation or concurrent own builds/tests during timing.

| Case | Series1 gain | Series2 gain | Positive processes each |
|---|---:|---:|---:|
|383 spread | +26.896% | +26.535% |6/6 |
|383 clustered | +16.959% | +17.001% |6/6 |
|192 spread | +52.239% | +52.762% |6/6 |
|192 clustered | +20.640% | +20.036% |6/6 |

Maximum median absolute A/A difference across these crossover cases/series was0.548%. No full-engine gain is inferred from this additional CPU gate.
