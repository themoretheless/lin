# Dictionary encoder CPU experiment

Limits costly escapes to two and aborts early for dense rows/unsafe size bounds. The catastrophic high-cardinality fallback is bounded, but metadata/probing overhead remains. Special four-row timing uses 256 operations/sample and exposes a real routing regression.

Frozen production reference e05ac74b (library source identical to a5e28e36). Twelve processes, two consecutive six-process series; rotated A/A/candidate with 24 samples per variant/case. Median process gain uses baseline A; A/A column is median absolute process deviation. No observations discarded. Buffers are preallocated/reused; allocation/drop, full WAL frame/CRC, file I/O, fsync, replay and peer runtimes are outside this measurement. Bytes below are vector sections only.

| Case | Series1 gain / wins / A-A | Series2 gain / wins / A-A | Reference / candidate bytes |
|---|---|---|---|
| default_1k | +18.00% / 6/6 / 1.46% | +23.07% / 6/6 / 2.05% | 238992 / 112542 |
| default_10k | -0.20% / 2/6 / 0.76% | -0.13% / 2/6 / 1.45% | 2539664 / 2539664 |
| high_cardinality | +1.29% / 5/6 / 1.77% | -0.45% / 1/6 / 0.46% | 520000 / 520000 |
| dense | -1.41% / 2/6 / 1.46% | -1.51% / 1/6 / 0.43% | 3076000 / 3076000 |
| zero | -1.64% / 1/6 / 1.02% | -1.65% / 2/6 / 1.72% | 8000 / 8000 |
| front_only_dictionary | -0.93% / 2/6 / 0.78% | -0.18% / 2/6 / 0.38% | 518832 / 518832 |
| probe_miss_majority_high | -0.96% / 3/6 / 1.83% | -1.26% / 1/6 / 0.92% | 518856 / 518856 |
| late_high_cardinality | -2.10% / 1/6 / 2.18% | -1.45% / 0/6 / 1.95% | 239280 / 239280 |
| probe_miss_dense | -2.99% / 0/6 / 0.69% | -1.95% / 1/6 / 2.08% | 3076000 / 3076000 |
| late_dense | -0.10% / 3/6 / 1.95% | +0.36% / 3/6 / 0.81% | 244912 / 244912 |
| half_high | -0.76% / 2/6 / 0.39% | -0.93% / 3/6 / 1.84% | 378720 / 378720 |
| special | -25.89% / 0/6 / 0.36% | -28.37% / 0/6 / 0.97% | 76 / 76 |

All measured outputs were compared as raw f32 bits, including negative zero and NaNs, using a trusted diagnostic parser. Boundary dimensions65535/65536/65537 are correctness preflights. This parser is not production untrusted-input decoder qualification. Exact outputs and hashes do not establish end-to-end durability or superiority over SQLite.

Production Rust remains unchanged. Next gate: strict existing codec5 reader, mixed-vector budget, migration/old reader behavior, actual frame sizes and full-engine native/Full durable A/A/B plus dense/high-cardinality/cold/resource controls. Tiny-path and fallback regressions must remain visible; default enablement is not justified by this CPU result.
