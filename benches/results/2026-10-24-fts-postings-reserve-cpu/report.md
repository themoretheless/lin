# FTS initial postings reserve — CPU and capacity gate

2026-10-24 baseline 13d0715. Production unchanged. Current add_posting, fts_texts and lowercased copied into reference.rs; rows cloned from actual default inserted documents outside timers. The standalone build helper matches fresh clean append-slab tokenization/posting maintenance with fields title/body and implicit snippet; not pending mutation or whole Db insertion. It also creates field strings inside timed work for every variant. Comparison uses the complete original-helper posting map as reference, not an independent tokenizer oracle.

Variants: baseline no reserve; variant 1 reserves min(rows,4096); variant 2 min(2*rows,8192). Reservation and all map allocations/destruction included in timing. Same lowercase, Unicode whitespace, sorted unique posting semantics. No production patch, dependency, worker, WAL or sync change.

Two series × six processes, 24 rotating samples/variant/case/process. Exact map equality before timing. Fixtures: real 1k/10k title/body, repeated two-term corpus, mixed Unicode/uppercase/supplementary characters/vertical tab/Unicode whitespace/implicit snippet. Finite fixture coverage, not universal correctness. Hashes, compiler build log, binary hash, raw samples and runner retained. No builds/tests overlap timing; unrelated host activity uncontrolled. CPU component only: no end-to-end, live peer or physical RSS claim.

| Series | Case | Reserve variant | Baseline us | Candidate us | Paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|---:|
| 1 | default_1k | 1 | 145.804 | 130.273 | +10.41% | 6/6 |
| 1 | default_1k | 2 | 145.804 | 131.460 | +10.03% | 6/6 |
| 1 | default_10k | 1 | 1385.417 | 1328.724 | +3.98% | 6/6 |
| 1 | default_10k | 2 | 1385.417 | 1267.339 | +8.31% | 6/6 |
| 1 | repeated_1k | 1 | 88.329 | 88.484 | -0.31% | 2/6 |
| 1 | repeated_1k | 2 | 88.329 | 88.322 | -0.33% | 1/6 |
| 1 | unicode_1k | 1 | 270.273 | 256.663 | +5.25% | 6/6 |
| 1 | unicode_1k | 2 | 270.273 | 256.946 | +4.87% | 6/6 |
| 2 | default_1k | 1 | 148.528 | 133.318 | +9.88% | 6/6 |
| 2 | default_1k | 2 | 148.528 | 134.120 | +9.02% | 6/6 |
| 2 | default_10k | 1 | 1444.589 | 1371.708 | +4.78% | 6/6 |
| 2 | default_10k | 2 | 1444.589 | 1308.870 | +8.84% | 6/6 |
| 2 | repeated_1k | 1 | 91.098 | 91.532 | -0.68% | 0/6 |
| 2 | repeated_1k | 2 | 91.098 | 91.536 | -0.59% | 0/6 |
| 2 | unicode_1k | 1 | 280.059 | 265.290 | +5.36% | 6/6 |
| 2 | unicode_1k | 2 | 280.059 | 267.099 | +4.85% | 6/6 |

## Capacity guard and decision

| Fixture | Unique terms | Baseline capacity | Reserve 1 capacity | Reserve 2 capacity |
|---|---:|---:|---:|---:|
| default_1k | 1005 | 1792 | 1792 | 3584 |
| default_10k | 10005 | 14336 | 14336 | 14336 |
| repeated_1k | 2 | 3 | 1792 | 3584 |
| unicode_1k | 1005 | 1792 | 1792 | 3584 |

Do not accept unconditional reserve: repeated 1k has only two terms, capacity rises 3 ->1792/3584 and time is flat/slightly worse. Capacity is the map's public element capacity, not resident bytes; no peak RSS qualification. Default 1k reserve 1 saves ~15 us (~10%), with the same final capacity; this does not prove a Db commit gain or close the SQLite gap. Variant 2 doubles final capacity at 1k without improving target CPU over variant 1.

A next candidate must make reservation conditional on bounded evidence of term cardinality and measure the cost of that evidence. It must preserve low-cardinality behavior and pending-edit semantics, then qualify full native/durable insertion with calibrated controls and exact postings. No unchanged unconditional-reserve engine run is justified by this resource guard. No workspace rerun for unchanged production source. Full eight-peer goal, durable SQLite gap and live MSSQL/Kusto qualification remain open.
