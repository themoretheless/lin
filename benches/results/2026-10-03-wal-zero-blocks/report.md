# Sparse WAL zero-block serialization

Retained optimization: sparse vector serialization checks the bitwise OR of
8-value blocks, skips all-positive-zero blocks and emits nonzero index/value
pairs from other blocks in the original order. Existing nonzero count and
codec-selection passes are unchanged. Negative zero, NaN payloads, infinity,
empty/present/missing vectors, checksum and frame-size/dimension bounds retain
the original representation. Full sync is unchanged.

Six alternating independent process pairs, 24 fresh fixtures per case, one
operation per sample, no warmup. Schema/preparation/fixture drop and validation
excluded. Existing fresh-fixture affected-count/readback gates passed. No
compilation/tests overlapped timing; host load and filesystem scheduling are
uncontrolled. Process medians aggregated before comparing pairs.

| Durable case | Baseline ms | Candidate ms | Paired median gain | Wins |
|---|---:|---:|---:|---:|
| Lin 1k | 3.435958 | 3.399646 | 0.89% | 4/6 |
| Lin 10k | 25.576646 | 24.505063 | 4.57% | 6/6 |
| SQLite 1k control | 1.777906 | 1.793292 | -0.99% | 2/6 |
| SQLite 10k control | 18.071958 | 18.208854 | -0.74% | 1/6 |

The supported improvement is large durable insertion. Small-batch improvement
is weak/inconclusive. Lin still loses to SQLite durable insertion at both sizes;
the full eight-peer objective is unproven. Native insertion and other peers are
not inferred from this experiment. Raw observations, binary/source hashes and
baseline/candidate source copies are retained.

Persistence unit tests (6) passed, including an extended bit-preservation case
with sparse values at block edges and in an incomplete final block. Full
workspace test results are recorded separately.

Final cargo test --offline --workspace passed. Benchmark baseline/candidate
compilation passed. Scoped rustfmt and git diff checks passed. Trailing empty
log lines were normalized; raw JSON observations are unchanged.
