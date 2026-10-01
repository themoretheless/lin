# Retained fused sorted FTS merge

The corrected sorted, unique pending-list invariant is preserved. FTS folding and pending-result lookup now compute `(base minus removals) union additions` with one output allocation and one merged traversal, removing the intermediate kept list. The old two-pass helpers remain test-only reference code.

## Prebuilt independent process comparison

Three independent process pairs, reversing the second pair; 10000-row resident delete/reinsert churn, 12 seconds per process. All final row counts, ID/URI lookups and exact lexical-search IDs/multiplicity were checked outside timing. Both binaries were built before the comparisons, and other source hashes matched.

| Pair | Correct baseline pairs/s | Fused pairs/s | Throughput change |
|---|---:|---:|---:|
| 1 | 138633.2 | 155977.7 | 12.51% |
| 2 | 140396.2 | 140485.1 | 0.06% |
| 3 | 130852.2 | 159524.4 | 21.91% |

Median paired throughput improvement: 12.51%. All three pairs won, but the second is nearly tied. Background load is uncontrolled; no statistical significance or universal causal percentage is claimed. This churn rate is not fresh-fixture single-delete latency or proof of beating peers.

## Current native deletion

One process, 12 fresh fixtures per case, affected-count/readback outside timing. No paired speedup attribution.

| Rows | Lin µs | SQLite µs |
|---|---:|---:|
| 1k | 4.729 | 3.104 |
| 10k | 11.937 | 10.271 |
| 100k | 20.041 | 16.833 |

## Current native bulk insertion

One process, 12 fresh fixtures, affected-count/readback checked outside timing; Appender flush inside timing. Native schemas/work differ as documented in the contract. The append-only insertion cases do not exercise pending FTS merges, so these fresh timings cannot establish a causal bulk speedup from this patch.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.107 | 0.814 | 1.204 |
| 10k | 12.281 | 10.426 | 10.394 |

Lin still loses native deletion and 10k bulk ingestion. A one-process 1k Appender win is insufficient to close the overall objective. MSSQL/Kusto live comparisons are still missing.

Validation: complete default workspace suite passed on this implementation. The fused merge matches the old two-pass reference for all 32768 combinations of small sorted sets. Existing out-of-order, 3000 deterministic random-edit, fold/codec, rollback and durable readback tests passed. Scoped rustfmt and git diff --check passed. All source variants, hashes, process logs and native observations are included.
