# Dictionary encoder CPU experiment

NEON nonzero masks with four entropy probes. Rejected: unsampled high-cardinality majority triggers full candidate encoding then fallback, causing >80% regression. Special four-row timings include zeros and cannot support performance claims.

Frozen production reference e05ac74b (library source identical to a5e28e36). Twelve processes, two consecutive six-process series; rotated A/A/candidate with 24 samples per variant/case. Median process gain uses baseline A; A/A column is median absolute process deviation. No observations discarded. Buffers are preallocated/reused; allocation/drop, full WAL frame/CRC, file I/O, fsync, replay and peer runtimes are outside this measurement. Bytes below are vector sections only.

| Case | Series1 gain / wins / A-A | Series2 gain / wins / A-A | Reference / candidate bytes |
|---|---|---|---|
| default_1k | +21.27% / 6/6 / 0.88% | +21.79% / 6/6 / 0.85% | 238992 / 112542 |
| default_10k | -0.11% / 3/6 / 0.61% | +1.54% / 6/6 / 1.37% | 2539664 / 2539664 |
| high_cardinality | +0.03% / 4/6 / 0.30% | -0.14% / 2/6 / 0.79% | 520000 / 520000 |
| dense | -0.80% / 0/6 / 0.64% | -0.25% / 3/6 / 0.78% | 3076000 / 3076000 |
| zero | -1.76% / 0/6 / 1.37% | -1.11% / 2/6 / 0.84% | 8000 / 8000 |
| front_only_dictionary | -0.05% / 3/6 / 0.53% | -0.36% / 2/6 / 0.95% | 518832 / 518832 |
| probe_miss_majority_high | -85.72% / 0/6 / 1.43% | -82.15% / 0/6 / 0.14% | 518856 / 518856 |
| late_high_cardinality | +0.12% / 3/6 / 3.59% | -3.34% / 1/6 / 2.73% | 239280 / 239280 |
| special | unqualified: zero-duration samples | unqualified: zero-duration samples | 76 / 76 |

All measured outputs were compared as raw f32 bits, including negative zero and NaNs, using a trusted diagnostic parser. Boundary dimensions65535/65536/65537 are correctness preflights. This parser is not production untrusted-input decoder qualification. Exact outputs and hashes do not establish end-to-end durability or superiority over SQLite.

Production Rust remains unchanged. Next gate: strict existing codec5 reader, mixed-vector budget, migration/old reader behavior, actual frame sizes and full-engine native/Full durable A/A/B plus dense/high-cardinality/cold/resource controls. Tiny-path and fallback regressions must remain visible; default enablement is not justified by this CPU result.
