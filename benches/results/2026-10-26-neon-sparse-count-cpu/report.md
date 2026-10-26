# Bounded NEON sparse-classifier count — CPU feasibility

Frozena5e28e36. Baseline reference.rs is copied byte-for-byte from the production
classifier plus scalar/NEON/parallel writer helpers. Candidate.rs changes only
the two nonzero-count loops and adds a hardware-gated integer NEON helper.
Float raw bits are compared with positive-zero bits; -0, subnormals, infinities
and every NaN payload count as nonzero. Four u32 accumulators count zero masks;
chunks<=2^20 floats bound all lanes/horizontal sums, with usize final totals.
Every SIMD load is inside a complete16-value block; scalar tails retain exact
semantics. Non-AArch64 keeps scalar counts. No cache, allocation or wire change.

Two independent six-process series,24 rotated A/A/candidate rounds,8 cases,
6912 timed compositions. Full vector representation choice plus the unchanged
writer and output allocation are timed; output destruction excluded. Every
result matches baseline bytes and codec. Input setup/oracles are outside timing.
Scalar bit oracle covers lengths0..1024 at four pointer offsets, positive/negative
zero, quiet/signalling NaNs, infinities, subnormals, threshold/width boundaries,
and2^20 chunk boundaries through2Mi+31 values. These checks cover the prototype,
not an untrusted decoder or a new format.

| Case | Paired gain s1 / s2 (%) | Faster processes s1 / s2 | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| default_1k | +27.937 / +28.096 | 6/6 / 6/6 | 1.147 / 0.990 |
| default_10k | +5.763 / +6.549 | 6/6 / 5/6 | 1.851 / 2.227 |
| small_127 | +29.614 / +29.481 | 6/6 / 6/6 | 0.201 / 0.103 |
| zero_1k | +40.073 / +40.629 | 6/6 / 6/6 | 0.212 / 0.958 |
| half_383 | +17.473 / +17.329 | 6/6 / 6/6 | 0.268 / 0.032 |
| half_384 | +47.097 / +46.720 | 6/6 / 6/6 | 0.047 / 0.180 |
| dense_1k | +46.637 / +46.969 | 6/6 / 6/6 | 1.074 / 2.492 |
| negative_zero_1k | +46.806 / +46.398 | 6/6 / 6/6 | 1.458 / 0.489 |

CPU feasibility passes for the measured corpus: default1k improves≈28%, all12
processes positive.10k uses the existing four-way classifier/writer and includes
its cold thread costs. Density crossover383/384 preserves the exact codec
choice. This does not establish commit latency, SQLite wins or whole-DB benefit;
full-engine/native/durable/replay/resource gates are still required. In particular
small-dimension call overhead and other architectures need their own controls.
No production source was changed by this CPU build/run; later integration is a
separate experiment. No samples/pairs removed.
