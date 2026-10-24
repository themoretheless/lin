# Production dictionary WAL writer: rejected

Baseline: `76e9bd32684a5633e7caace606b7ba0bd8e7287f`. The measured candidate is preserved in `candidate-persist.rs`; production `src/persist.rs` was restored byte-for-byte to the baseline. Automatic codec-5 writing remains disabled.

## Decision

Reject: the targeted durable 1k workload regressed in both independent series. Smaller frames and the earlier positive CPU-only selector experiment did not translate into end-to-end durable throughput. Do not retry this implementation unchanged.

## Measurements

Two independent series, six alternating baseline/candidate process pairs each, native and durable 1k/10k, 24 observations per case. Existing Full sync, embeddings, FTS, indexes and limits retained. SQLite is an unchanged control. Positive paired gain means faster.

| Case | Series 1 paired gain | Series 2 paired gain | Candidate wins S1/S2 |
|---|---:|---:|---:|
| insert_bulk_1k/lin | -1.838% | -1.684% | 3/6, 1/6 |
| insert_bulk_1k/sqlite | +2.244% | -1.075% | 4/6, 2/6 |
| insert_bulk_10k/lin | +26.044% | -0.003% | 4/6, 3/6 |
| insert_bulk_10k/sqlite | +24.808% | +0.460% | 4/6, 4/6 |
| durable_insert_1k/lin | -3.295% | -4.869% | 3/6, 2/6 |
| durable_insert_1k/sqlite | +0.303% | +2.149% | 3/6, 5/6 |
| durable_insert_10k/lin | -1.235% | +1.048% | 1/6, 5/6 |
| durable_insert_10k/sqlite | +0.999% | +0.942% | 4/6, 5/6 |

Series 1 native 10k had substantial common-mode timing variation: Lin +26.0% and unchanged SQLite +24.8%; the effect did not repeat in series 2. All raw observations and outliers are retained. Ratios of aggregate medians must not replace paired gains. No memory gain is claimed; a dedicated RSS gate was not run after the performance rejection.

## Correctness evidence

Default workspace, writer tests and candidate/helper builds passed (`candidate-checks.json`, `writer-tests.log`). Actual codec-5 process-exit fixtures recovered 3000/4000 acknowledged rows with bit-exact vectors. A frozen pre-capability reader rejected the new frames without changing any fixture file. Ten representative unacknowledged-tail truncations recovered the preceding 3000 acknowledged rows, and complete-frame corruption was rejected without mutation. This tests process exit and modeled future torn tails, not physical power loss. See `crash-verification.json`; initial and final source/fixture evidence are retained separately.

The first real data frame was 210531 bytes including framing. Byte savings alone are insufficient to accept the writer. Source and binary hashes are in baseline/candidate provenance files. The reader capability already on main is retained.

## Remaining objective

Durable 1k remains a SQLite gap. Current-source peer refresh and MSSQL/Kusto live qualification remain outstanding. This rejected experiment establishes no new superiority over any named peer.
