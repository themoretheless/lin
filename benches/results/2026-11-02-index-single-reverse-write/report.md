# Single reverse-map write: no demonstrated uniform component gain

Local ARM64 lib-test component only; compiler and binary hashes in the archived status. No external engine or full DB operation measured. All108 processes/864 samples recomputed independently;1,728,000 helper calls. No build overlap with these local timings. Unrelated host activity uncontrolled.

| Series | Rows | Gain % | AA % | Positive pairs |
|---|---:|---:|---:|---:|
| 1 | 1000 | +4.527 | 2.241 | 5/6 |
| 1 | 10000 | +3.439 | 4.097 | 6/6 |
| 1 | 100000 | +0.719 | 1.273 | 5/6 |
| 2 | 1000 | +4.491 | 4.063 | 6/6 |
| 2 | 10000 | +4.680 | 2.286 | 5/6 |
| 2 | 100000 | +3.225 | 3.346 | 5/6 |

Index tests:9 passed /1 ignored before adding the explicit ignored measurement fixture. Differential oracle preserves the original helper body verbatim, and checks errors, canonical reverse keys and posting representation/membership over mixed keys and mutation sequences. Component timings use both helpers in one test executable, with final complete forward/reverse state assertions after every sample. Function-pointer/cfg(test) effects limit extrapolation to production.

No reliable all-size/two-series improvement: several gains are below AA and most windows lack6/6 positive pairs. Candidate not adopted; runtime src/index.rs restored byte-for-byte to60e64559e, candidate source and tests retained in candidate-index.rs. No unchanged retry or numeric waiver. The running full Linux matrix451fd7a25 excludes this experiment and remains authoritative for that separate candidate.
