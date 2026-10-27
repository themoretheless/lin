# Calibrated reusable-text batch component qualification

Declared protocol in protocol.md, unchanged candidate from exploratory run. Twelve fresh processes, two series of6; four cases; baseline/control/candidate each12 rotated samples =1728 batch observations. Exact f32-bit checks and all original controls retained. Actual production row_embed_text/embed_batch versus same fill/finish with one batch scratch borrow and reusable joined-text String. Source restored; source/helper/gate/binary hashes retained, final binary hash checked after12 processes.

| Case | Series | Gain % | Absolute A/A % | Positive processes | Gate |
|---|---|---:|---:|---:|---|
| default_10k | series1 | 7.584 | 0.545 | 6/6 | True |
| default_10k | series2 | 8.242 | 0.680 | 6/6 | True |
| default_1k | series1 | 9.634 | 0.545 | 6/6 | True |
| default_1k | series2 | 10.112 | 0.277 | 6/6 | True |
| single_field_1k | series1 | 0.645 | 0.503 | 4/6 | True |
| single_field_1k | series2 | 0.706 | 0.364 | 5/6 | True |
| unicode_1k | series1 | 0.813 | 0.159 | 6/6 | True |
| unicode_1k | series2 | 1.114 | 0.184 | 6/6 | True |

Component gate passes: both default targets6/6 positive and above A/A in each series; single-field/Unicode controls within unchanged non-regression bands. This is warm local CPU component evidence, not full-engine native/durable, cold start, memory/RSS, custom backend semantics or peer superiority. Host activity uncontrolled. The initial A/B experiment is separately retained; this declared calibration adds an identical baseline control and independent second series, not a selected replacement for losers.

Next: integrate a default-compatible batch capability preserving custom Embedder batch overrides and existing wrong-length fallback, existing vectors, empty text, exact vectors and full insertion/readback, then measure actual native/Full durable fixtures versus peers. Neither a public API addition nor production integration occurs in this report. Full eight-peer goal remains false.
