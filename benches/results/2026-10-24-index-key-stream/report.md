# Stream one index key at a time — component qualification

2026-10-24 baseline2b800d6, including accepted once-per-slab bitmap sample. Candidate removes only temporary Vec<IndexKey>, constructs the owned key immediately before each insertion, and retains keyed_rows filtered-position vector, one captured now, sample-once bitmap logic, row order and unique error formatting. No public/format/sync/index-definition changes. Both source snapshots and exact source/binary/compiler metadata retained.

Existing per-row reference oracle and full default workspace passed for candidate. The oracle compares errors and index state, including partial unique failures, partial predicates, missing sample fields, isolated late boolean, sample cardinalities and dynamic10000-key boundary. key_of is pure with respect to index mutation. No induced OOM/panic/concurrency behavior qualification.

Actual index worker: six cases, two/three-field keys, explicit sampling enabled/disabled, low/high cardinality,1k/10k. Empty seed clone before timer, insert_slab_batch timed, exact posting reconstruction from source rows outside timer, output destruction excluded. Two independent series ×six alternating process pairs, twelve samples/case/process. No runner builds/tests overlap timing; unrelated host activity uncontrolled. No full-Db, p99 or RSS claim.

| Series | Fixture | Baseline ms | Stream ms | Median paired gain | Positive pairs |
|---:|---|---:|---:|---:|---:|
| 1 | bitmap_1k | 0.065239 | 0.070000 | -7.116% | 0/6 |
| 1 | bitmap_10k | 0.613323 | 0.642677 | -3.495% | 0/6 |
| 1 | standard_card100 | 0.091625 | 0.093156 | -1.444% | 2/6 |
| 1 | disabled_1k | 0.057573 | 0.061645 | -4.729% | 0/6 |
| 1 | disabled_10k | 0.546979 | 0.578375 | -5.616% | 0/6 |
| 1 | spilled_3fields | 0.123583 | 0.119063 | +2.436% | 5/6 |
| 2 | bitmap_1k | 0.066177 | 0.067739 | -1.656% | 2/6 |
| 2 | bitmap_10k | 0.618760 | 0.647208 | -5.559% | 1/6 |
| 2 | standard_card100 | 0.092490 | 0.093615 | +1.193% | 4/6 |
| 2 | disabled_1k | 0.057751 | 0.061166 | -7.782% | 0/6 |
| 2 | disabled_10k | 0.543490 | 0.576865 | -6.325% | 0/6 |
| 2 | spilled_3fields | 0.126198 | 0.119167 | +5.178% | 6/6 |

| Fixture | IndexKey sizeof | Rows | Eliminated key-Vec element storage projection |
|---|---:|---:|---:|
| bitmap_1k | 80 | 1000 | 80000 bytes |
| bitmap_10k | 80 | 10000 | 800000 bytes |
| standard_card100 | 80 | 1000 | 80000 bytes |
| disabled_1k | 80 | 1000 | 80000 bytes |
| disabled_10k | 80 | 10000 | 800000 bytes |
| spilled_3fields | 80 | 1000 | 80000 bytes |

The final column is only the removed Vec element-storage projection for these all-matching fixtures. It is not measured peak live allocation or RSS; spilled key backing, reverse maps, posting allocation/lifetimes and allocator effects are additional. No memory-performance claim justifies retaining the global policy despite CPU regression.

## Decision: reject global streaming

Two-field disabled path regresses in all six pairs of both series at both sizes. Three-field spilled keys improve (+2.44% /+5.18%,5/6 /6/6), but this does not prove an arity-specific engine policy. No unchanged global-candidate full-engine/peer run warranted. Any future arity dispatch must preserve default-path performance, independently validate its added branch/lifetime costs and qualify full engine; no such dispatch ships here.

Production index.rs restored byte-for-byte and all baseline core hashes verified. Previously accepted sample-once optimization remains intact. No source change accepted, no full-Db acceleration or peer victory inferred from isolated spilled-key results. SQLite durable gap and missing live MSSQL/Kusto remain open; full eight-peer objective unchanged.
