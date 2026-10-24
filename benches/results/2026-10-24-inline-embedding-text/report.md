# Inline embedding text: rejected

Baseline production source `711c0a5fdbb1d1ddf449db14c4203448e1732483`; pinned compare baseline reused from the earlier reader-only source after verifying every core source hash and executable hash matched the current unmodified production source. Candidate source snapshot and separate hashes retained. Source embed.rs restored byte-for-byte; no production change ships.

## Candidate and correctness

Replace private Cow text preparation with borrowed / existing 22-byte InlineStr / String variants. Multi-field joins fitting 22 UTF-8 bytes avoid heap string allocation; long joins retain owned String and single-field joins remain borrowed. Preserve leading-empty and trailing-present-empty separator behavior. Public Embedder API, feature hashing, embedding bits, FTS, indexes, Full sync, WAL and quotas unchanged. This differs from the earlier packing-cache experiment: it affects actual embedding text concatenation.

Twelve focused embedding tests and full default workspace passed. New 512-combination test compares the old concatenation algorithm and every embedding f32 bit, covering absent/empty/short/22-byte/23-byte/Unicode fields and inline/owned paths. Release build succeeded. No optional-platform or physical power-loss qualification inferred.

## Timing

Two independent six-pair alternating baseline/candidate process series. Native and Full durable 1k/10k; 24 fresh measured samples per case, one operation, no warmup. Exact readback outside timing, SQLite unchanged control. No runner-initiated builds/tests overlap timing; unrelated host load uncontrolled. Positive value means paired median speedup.

| Case | Series 1 | Series 2 | Faster pairs S1/S2 |
|---|---:|---:|---:|
| insert_bulk_1k/lin | -1.218% | +1.224% | 2/6, 5/6 |
| insert_bulk_1k/sqlite | -1.690% | -0.010% | 0/6, 3/6 |
| insert_bulk_10k/lin | -1.265% | +0.865% | 0/6, 4/6 |
| insert_bulk_10k/sqlite | +0.405% | +0.784% | 3/6, 4/6 |
| durable_insert_1k/lin | +5.426% | -4.889% | 4/6, 3/6 |
| durable_insert_1k/sqlite | +0.381% | -5.104% | 4/6, 1/6 |
| durable_insert_10k/lin | +2.390% | -9.635% | 4/6, 2/6 |
| durable_insert_10k/sqlite | +0.472% | -8.411% | 4/6, 0/6 |

Reject: first-series durable gains do not repeat. Second series has material control/absolute timing shifts (durable 1k Lin -4.89%, SQLite -5.10%; 10k Lin -9.64%, SQLite -8.41%), so no isolated causal regression is assigned; it still does not support retaining the first-series gain. Native results are mixed too. All raw observations/outliers retained; no selected series discarded. No dedicated RSS or server refresh run warranted for the rejected candidate. Saving small temporary string allocations is not established as an end-to-end latency benefit here.

Durable 1k SQLite remains a gap. Current accepted read/native results remain on unchanged production source, and absent MSSQL/Kusto qualification stays open. Full goal incomplete.
