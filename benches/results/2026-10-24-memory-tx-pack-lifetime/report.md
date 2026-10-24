# Rejected: earlier release of memory transaction WAL markers

Baseline source `311e11f`; exact before/candidate source snapshots, pinned binary hashes and raw measurements retained. Candidate kept Transaction.packs only when Db.persist existed, releasing memory statement markers immediately. Five transaction lifecycle tests including durable reopen passed; no full workspace rerun, because performance acceptance failed and source was restored byte-for-byte.

## Correction to the initial hypothesis

The initial hypothesis incorrectly assumed data-bearing WAL packs accumulated for memory bulk inserts. Source inspection in Stmt::Insert shows memory inserts already emit `Pack::Batch { packs: Vec::new() }`, while only durable insertion calls rows_to_insert_cols. Thus this candidate releases empty markers, not stored row/embedding payloads. Transaction rollback state is independent. This source fact and the measurements rule out retained row payloads as the memory-transaction bottleneck in this fixture.

## Measurements

Two series, each six alternating baseline/candidate process pairs at each of 1k/10k/100k. Lin/SQLite/DuckDB native API insertion, 12 fresh fixtures per case/process, one timed operation, preparation/schema/validation/cleanup excluded. Every run complete and exact-validated; --require-wins exit 3 records actual peer losses. No builds/tests initiated by this runner overlap timing; host activity otherwise uncontrolled. All raw outliers retained (including candidate Lin ~289ms and baseline DuckDB ~208ms at 100k); no exclusions or causal explanation asserted.

1k/10k use a single prepared run and do not exercise the changed transaction branch; their shifts are control/binary-layout/host observations, not evidence of a causal optimization. 100k uses ten prepared chunks inside one memory transaction.

| Series | Rows | Lin paired gain | Candidate wins | Baseline Lin ms | Candidate Lin ms | SQLite control gain | DuckDB control gain |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1000 | -0.460% | 3/6 | 0.775 | 0.773 | 1.802% | 5.613% |
| 1 | 10000 | 2.741% | 5/6 | 9.539 | 9.307 | 1.130% | 0.499% |
| 1 | 100000 | -0.063% | 3/6 | 110.163 | 107.517 | -0.511% | 0.839% |
| 2 | 1000 | -0.692% | 2/6 | 0.746 | 0.748 | -0.043% | -0.028% |
| 2 | 10000 | -0.267% | 3/6 | 8.837 | 8.993 | 0.389% | 0.746% |
| 2 | 100000 | -2.961% | 2/6 | 103.913 | 106.455 | -0.746% | 0.006% |

## RSS check

Three additional alternating pairs after timing completed, `/usr/bin/time -l` on macOS around each pinned worker, each request 100k rows and 12 fresh samples. Full worker peak includes parsing, store, canonicalization/output and allocator effects; not an insertion-phase live allocation counter. All rows exactly checked by the worker. Raw stderr/rusage and rss.json retained. Median maximum RSS: baseline 897.516 MiB, candidate 932.656 MiB. Candidate was higher in 3/3 pairs; no memory saving established and no causal attribution of this peak shift.

## Decision

Reject. Target 100k paired gains are -0.063% (3/6 wins) then -2.961% (2/6), with no RSS saving. Source restored exactly to baseline, no runtime change landed. The new chunked-100k-insert-profile measures actual stages for the next hypothesis. Existing 100k DuckDB and durable SQLite gaps, plus unqualified MSSQL/Kusto, remain open.
