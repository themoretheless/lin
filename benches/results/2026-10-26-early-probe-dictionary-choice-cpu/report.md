# Dictionary encoder CPU experiment

Moves row-count/size hints before metadata scans; uses NEON probes and rejects obvious dense/zero controls early. Adds unsampled high/dense outliers at row998 (row999 was already probed). CPU target improves, but probe-miss fallback and tiny routing still regress. Not accepted as a default production writer.

Frozen production reference e05ac74b (library source identical to a5e28e36). Twelve processes, two consecutive six-process series; rotated A/A/candidate with 24 samples per variant/case. Median process gain uses baseline A; A/A column is median absolute process deviation. No observations discarded. Buffers are preallocated/reused; allocation/drop, full WAL frame/CRC, file I/O, fsync, replay and peer runtimes are outside this measurement. Bytes below are vector sections only.

| Case | Series1 gain / wins / A-A | Series2 gain / wins / A-A | Reference / candidate bytes |
|---|---|---|---|
| default_1k | +20.35% / 6/6 / 0.33% | +21.19% / 6/6 / 0.42% | 238992 / 112542 |
| default_10k | +0.38% / 4/6 / 1.58% | +0.70% / 5/6 / 0.47% | 2539664 / 2539664 |
| high_cardinality | +0.08% / 6/6 / 0.08% | +0.05% / 5/6 / 0.05% | 520000 / 520000 |
| dense | +0.25% / 5/6 / 0.67% | +0.48% / 5/6 / 0.28% | 3076000 / 3076000 |
| zero | -0.03% / 1/6 / 0.02% | -0.07% / 2/6 / 0.18% | 8000 / 8000 |
| front_only_dictionary | -0.03% / 2/6 / 0.03% | -0.12% / 2/6 / 0.37% | 518832 / 518832 |
| probe_miss_majority_high | -1.07% / 0/6 / 0.09% | -1.07% / 0/6 / 0.05% | 518856 / 518856 |
| late_high_cardinality | -0.26% / 1/6 / 0.20% | -0.44% / 2/6 / 0.20% | 239280 / 239280 |
| probe_miss_dense | -1.26% / 0/6 / 0.24% | -0.99% / 1/6 / 0.41% | 3076000 / 3076000 |
| late_dense | -0.42% / 2/6 / 0.40% | -0.18% / 2/6 / 0.25% | 244912 / 244912 |
| half_high | -0.16% / 1/6 / 0.20% | -0.11% / 2/6 / 0.14% | 378720 / 378720 |
| hidden_late_high | +21.27% / 6/6 / 0.14% | +17.71% / 6/6 / 0.39% | 239272 / 112951 |
| hidden_late_dense | +26.26% / 6/6 / 0.27% | +21.96% / 6/6 / 1.35% | 244904 / 118583 |
| special | -17.76% / 0/6 / 0.31% | -15.17% / 0/6 / 0.00% | 76 / 76 |

All measured outputs were compared as raw f32 bits, including negative zero and NaNs, using a trusted diagnostic parser. Boundary dimensions65535/65536/65537 are correctness preflights. This parser is not production untrusted-input decoder qualification. Exact outputs and hashes do not establish end-to-end durability or superiority over SQLite.

Production Rust remains unchanged. Next gate: strict existing codec5 reader, mixed-vector budget, migration/old reader behavior, actual frame sizes and full-engine native/Full durable A/A/B plus dense/high-cardinality/cold/resource controls. Tiny-path and fallback regressions must remain visible; default enablement is not justified by this CPU result.
