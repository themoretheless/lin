# Borrowed batch identity uniqueness sets

Baseline source 8980f19 (production unchanged from retained 4926d9c). Candidate changes temporary batch id/uri sets from FxHashSet<Arc<str>> to FxHashSet<&str>, referencing stable strings in already constructed rows. It avoids both temporary text_shared and owned-set Arc increments/decrements for id/uri. Row defaults/hash computation and validation ordering are unchanged; body hash still uses its existing temporary Arc. Sets are explicitly dropped before embedding/moving/mutating the slab. No unsafe code, public Row/Cell/API, permanent map ownership, WAL, format, embedding model or durability change.

This differs from the earlier rejected borrowed-row-text experiment: that retained owned Arc uniqueness sets and only removed temporary references. Here the sets themselves are borrowed. Existing 61 execution tests pass, covering uniqueness, transactional rollback and bulk boundary behavior. Full cargo test --workspace passed on 2026-10-06, exit 0 in workspace-status.json. Default-feature workspace coverage does not claim every optional feature was executed. The initial run lost its process handle and log ended at GPU binary startup, so it was not counted as a full pass; its incomplete log is preserved. A verified fresh run completed successfully. The earlier startup sample is diagnostic only.

Two independent six-pair series, same pinned binaries, alternating process order, 24 fresh fixtures per case, one operation/sample, no warmup. Exact fixture values verified outside timing. No builds/tests overlap timing. Binary hashes in each round. Percentages are medians of paired percent improvement, wins are process medians. Uncontrolled host load/clocks; early first-run process startup was slow and sampled latencies are separate from launch time. No causal attribution to startup behavior.

| Case | First paired gain / wins | Repeat paired gain / wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | +2.119% / 6/6 | +3.011% / 6/6 |
| compare/insert_bulk_1k/sqlite | +0.902% / 5/6 | +0.521% / 5/6 |
| compare/insert_bulk_10k/lin | +2.558% / 5/6 | +6.629% / 6/6 |
| compare/insert_bulk_10k/sqlite | +0.429% / 5/6 | -0.265% / 2/6 |
| compare/durable_insert_1k/lin | +1.052% / 4/6 | +0.905% / 4/6 |
| compare/durable_insert_1k/sqlite | -0.271% / 3/6 | +2.232% / 5/6 |
| compare/durable_insert_10k/lin | +0.866% / 3/6 | +0.736% / 5/6 |
| compare/durable_insert_10k/sqlite | -0.084% / 3/6 | -0.646% / 2/6 |

Native improvements reproduce: 1k 6/6 wins both rounds, 10k 5/6 then 6/6. SQLite controls shift less than Lin in these cases. Durable gains are small and mixed, particularly 1k where repeat SQLite control improves more than Lin; no isolated durable speedup is claimed. Retention is based on repeated native gains and no observed repeatable durable regression, with full workspace tests passed. Production candidate retained. No per-core or universal peer superiority claim.

Remaining SQLite gaps: native 1k candidate aggregate Lin 0.819260 vs SQLite 0.800906 ms first and 0.808729 vs 0.793271 ms repeat. Full durable 1k Lin 2.606136 vs SQLite 1.372854 ms first, 2.761687 vs 1.277396 ms repeat. Native gap is smaller but not closed. The all-named-peer goal remains incomplete, including MSSQL/Kusto availability and write scope/equivalence limits of API comparisons.
