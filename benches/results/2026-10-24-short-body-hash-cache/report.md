# Rejected unconditional short-body hash cache bypass

Baseline production source `20739a4` (same production code as `311e11f`). Candidate changed cached_content_hash to directly call bit-identical content_hash_arc whenever body.len() <=64, affecting all callers and batch sizes. Exact snapshots, hashes, tests and raw results retained. Hash checks covered empty/Unicode/63/64/65-byte/long bodies, cache churn and retained long-body cache reuse; they passed. No full workspace rerun because this variant was superseded before adoption.

Two independent series of six alternating process pairs per size, Lin/SQLite/DuckDB native API insertion, 12 fresh samples, 1k/10k/100k. All results exact-validated; pinned release binaries, setup/preparation/schema/validation/cleanup excluded; no competing builds/tests initiated by this runner. Controls and raw outliers retained; unrelated host activity uncontrolled. API costs are not identical; see benchmark-contract.md.

| Series | Rows | Lin paired gain | Candidate wins vs baseline | Candidate Lin ms |
|---|---:|---:|---:|---:|
| 1 | 1000 | -4.244% | 2/6 | 0.780 |
| 1 | 10000 | 12.089% | 6/6 | 8.067 |
| 1 | 100000 | 12.553% | 6/6 | 91.958 |
| 2 | 1000 | -3.948% | 1/6 | 0.771 |
| 2 | 10000 | 11.600% | 6/6 | 7.806 |
| 2 | 100000 | 15.879% | 6/6 | 88.524 |

Candidate wins against DuckDB at 100k in all twelve processes, but 1k shows negative paired effects in both series. Warm 1k unique bodies fit the previous ~1023-entry cache, so unconditional recomputation discards useful reuse; this is a source-based explanation, not isolated attribution of all observed drift.

Diagnostic guard workers retained all normal features and exact six-field fixtures, modifying only body generation outside timing. Six alternating pairs, 12 fresh samples each, repeated short body and repeated 256-byte body at 1k/10k. Every row validated independently by Python; exact worker snapshot/hashes retained. Guards show no consistent repeat-body regression (4/6 wins in each, small positive paired medians); these Lin-only controls do not establish peer victories. See guard-summary.json and guard-observations.json.

Decision: do not adopt the unconditional change. Follow-up bulk-short-body-hash keeps cached behavior for small batches and updates, applying direct hashing only inside bulk construction at >=4096 records with <=64-byte bodies. The size thresholds are engineering heuristics, not established universal crossovers. All rejected and noisy runs remain available.
