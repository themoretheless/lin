# Scalar-index position move: rejected

Three alternating independent prebuilt process pairs, 16 fresh fixtures per case. Setup/destruction and readback validators are outside timing. The candidate moves reverse/forward positions without recreating the key; it preserves remove/reinsert posting order.

| Pair | Rows | Baseline Lin µs | Candidate Lin µs | Baseline SQLite µs | Candidate SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | 1k | 10.146 | 10.438 | 4.229 | 7.708 |
| 1 | 10k | 17.230 | 16.709 | 11.375 | 13.146 |
| 1 | 100k | 28.250 | 30.229 | 21.084 | 19.166 |
| 2 | 1k | 8.375 | 9.500 | 5.417 | 5.750 |
| 2 | 10k | 14.396 | 13.041 | 13.479 | 12.750 |
| 2 | 100k | 22.604 | 27.188 | 18.666 | 18.521 |
| 3 | 1k | 8.438 | 8.896 | 3.896 | 4.500 |
| 3 | 10k | 14.604 | 13.229 | 10.250 | 10.104 |
| 3 | 100k | 24.104 | 19.270 | 16.875 | 18.895 |

The 10k case won all three pairs, but 1k lost all three and 100k lost two of three. Overall native-delete wins were not demonstrated. The candidate was reverted. Background load is uncontrolled, so no causal regression percentage is claimed. The workspace suite and all sampled readback checks passed. Exact sources/hashes and raw runs remain. The prior inline-key/search-bound optimization remains retained.
