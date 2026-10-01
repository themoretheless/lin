# Streaming FTS removal: rejected

The candidate removes the temporary sorted/deduplicated token vector from FtsIndex::remove_row. It processes fields and tokens directly, relying on idempotent pending removal sets. Contextual Unicode str::to_lowercase semantics are preserved. The baseline and candidate were built from one immutable source snapshot, with only this method differing. Production files were not changed.

Nine focused FTS tests passed, including duplicate tokens, edit cancellation, descending/random removals, folds and codec roundtrip. Builds and tests completed before measurements. Three independent prebuilt process pairs, alternating order, 16 fresh fixtures per case, one timed operation per fixture. Existing count/readback validators are outside timing. Background machine load remains uncontrolled; no statistical significance is asserted.

| Pair | Rows | Baseline Lin µs | Candidate Lin µs | Reduction | Baseline SQLite µs | Candidate-process SQLite µs |
|---|---|---:|---:|---:|---:|---:|
| 1 | 1k | 7.979 | 6.458 | 19.1% | 5.042 | 3.083 |
| 1 | 10k | 14.291 | 14.354 | -0.4% | 11.271 | 12.250 |
| 1 | 100k | 18.375 | 24.770 | -34.8% | 18.958 | 18.750 |
| 2 | 1k | 9.896 | 8.166 | 17.5% | 7.333 | 4.188 |
| 2 | 10k | 14.291 | 14.854 | -3.9% | 11.792 | 10.021 |
| 2 | 100k | 24.167 | 24.521 | -1.5% | 16.104 | 18.750 |
| 3 | 1k | 4.708 | 7.458 | -58.4% | 3.062 | 3.542 |
| 3 | 10k | 14.146 | 14.354 | -1.5% | 10.521 | 12.396 |
| 3 | 100k | 19.625 | 24.041 | -22.5% | 16.166 | 15.938 |

Candidate wins: {'1k': 2, '10k': 0, '100k': 0}. This does not establish a consistent improvement across sizes or a SQLite victory. Candidate rejected; the existing FTS removal remains. Raw observations, build/test logs and source hashes are retained. Build-only Cargo invocations returned exit 1 because the intentional filter matched no cases; compilation succeeded and actual six measured processes completed successfully.
