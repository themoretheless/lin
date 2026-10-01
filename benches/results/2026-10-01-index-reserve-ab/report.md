# Reverse-index reservation experiment

Six independent prebuilt process pairs, alternating order; 12 fresh fixtures per phase.

| Pair | Baseline full ms | Reserve full ms | Improvement |
|---|---:|---:|---:|
| 1 | 18.019 | 38.194 | -112.0% |
| 2 | 21.584 | 18.726 | 13.2% |
| 3 | 38.874 | 48.327 | -24.3% |
| 4 | 24.158 | 24.104 | 0.2% |
| 5 | 22.378 | 32.946 | -47.2% |
| 6 | 26.761 | 29.453 | -10.1% |

Not retained: only two of six full-insert pairs won, with strongly unstable timings. Reservation was reverted. No causal regression percentage or peer win is claimed. Focused index tests passed; the source variants and raw observations remain.
