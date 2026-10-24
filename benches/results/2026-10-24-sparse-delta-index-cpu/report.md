# Sparse delta indexes — CPU/byte gate

2026-10-24 baseline a55132c. New format proposal only: replace each absolute u32 sparse coordinate with unsigned LEB128 gap from previous coordinate-plus-one, followed by unchanged exact u32 f32 bits. Positive gaps, per-row reset; None/Some(empty) and dimension/count headers unchanged. This differs from prior fixed-u16 narrow-coordinate and value-dictionary proposals. No production writer/reader/codec ID introduced or selected. Existing codec 5 dictionary reader is not repurposed.

Original serial encoder copied from current persist.rs into reference.rs. Candidate preserves the eight-float zero-block test and traversal order, changes only coordinate emission. Independent diagnostic re-expansion validates positive gaps, checked coordinate addition, dimension bounds, bounded u32 varint and complete consumption, then reconstructs original legacy byte stream exactly. Preflight includes None/Some(empty), positive/negative zero, NaN payloads, infinities, tails and gap transitions across 127/128/16383/16384 at dimension 65539. This is finite valid-input roundtrip, not production corrupt-input security or allocation-budget qualification. The diagnostic decoder is assertion-based and is not safe production error handling.

Real default embeddings at 1k/10k from actual Db fixture, forced dense control and special boundary fixture. Two independent series ×six processes, 24 rotating samples per variant/case/process, reused buffers with capacity prepared outside timing. Only serial vector encoding measured: not classification, full frame, parallel 10k, checksums, disk or sync. Buffer reuse and fixed ordering differ from Db lifecycle. No runner build/test overlap; unrelated host activity uncontrolled. All raw outliers retained; percentages are medians of paired process changes.

| Fixture | Legacy vector bytes | Delta vector bytes | Reduction |
|---|---:|---:|---:|
| default_1k | 238992 | 152370 | 36.24% |
| default_10k | 2539664 | 1617290 | 36.32% |
| dense | 6152000 | 3848000 | 37.45% |
| boundaries | 124 | 94 | 24.19% |

| Series | Fixture | Legacy us | Delta us | Paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 226.289 | 240.722 | -5.992% | 1/6 |
| 1 | default_10k | 2799.549 | 2954.711 | -5.498% | 0/6 |
| 1 | dense | 537.016 | 975.607 | -80.363% | 0/6 |
| 1 | boundaries | 8.957 | 8.683 | -0.353% | 1/6 |
| 2 | default_1k | 233.157 | 250.518 | -7.119% | 1/6 |
| 2 | default_10k | 2883.357 | 3022.651 | -5.281% | 0/6 |
| 2 | dense | 545.115 | 1006.729 | -80.448% | 0/6 |
| 2 | boundaries | 9.217 | 8.902 | -0.405% | 2/6 |

## Next gate, not acceptance

Default 1k vector section shrinks by 86622 bytes (36.24%), but serial encoder costs ~14–17 us more and loses 5/6 process pairs in each series. Forced-dense encoding is much slower; production selection would need cost/size fallback, and existing sparse classifier behavior must be explicitly specified. Default 10k is serial component evidence only; existing production parallelization cannot be assumed to have the same effect.

For reference, current measured 1k full legacy payload is336968 bytes. If exactly this vector section were replaced and all other fields were unchanged, projected payload would be250346 bytes, ~25.7% smaller. This is an arithmetic projection, not an emitted or replayed new-codec full frame and not a latency prediction. A new codec/reader contract would add separate framing/compatibility considerations.

The small target CPU penalty does not alone decide total disk cost. Next: actual encode+write+Full-sync feasibility with old/new identical full-fixture inputs, calibrated A/A and all costs included. Only if that passes should a versioned bounded production decoder, backward rejection/nonmutation, crash/replay, optional-platform and calibrated full-engine gates be implemented. Do not claim a DB speedup or shipping compression from this report.

Production source unchanged and source hashes verified after all runs. No workspace rerun for diagnostic-only source, no peak RSS/power-cut/live peer qualification. SQLite durable gap and live MSSQL/Kusto remain open; full eight-peer objective unchanged.
