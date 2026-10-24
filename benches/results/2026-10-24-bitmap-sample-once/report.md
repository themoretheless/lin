# Sample bitmap cardinality once per index slab

2026-10-24 baseline 3e7ab10. Candidate production change in src/index.rs: compute the first-up-to-100-row sample once after partial filtering has established a nonempty insertion. Retain original per-row boolean override and the current forward-key-count threshold (<=16 below/equal10000 keys, <=32 above). A boolean in the sample still enables bitmap for all rows as before; missing sample values and empty sample retain original rules. Sampling uses the original entire input slab, including rows excluded by a partial predicate. No posting encoding, public API, WAL/sync, default index flag or automatic bitmap enablement changes.

Original per-row method remains under cfg(test), with the old full slab algorithm as a reference. Functional comparison checks exact errors, reverse maps, forward keys, posting-list representation and positions after success or partial unique failure. Matrix includes 0/1/2/99/100/101/128 rows, 0/1/16/17/32/33 sample cardinalities, missing values, booleans, unique/nonunique, partial predicates and prepopulated >10000-key index. Full default workspace and focused oracle tests passed (logs/checks.json). Additional branch-isolation checks, if present, are recorded in final-verification.json.

## Actual index-operation performance

Both release workers built fresh from saved baseline/candidate index.rs, identical compiler and worker; exact core/binary hashes retained. Two independent series ×six alternating process pairs; twelve fresh empty-index samples per case/process. Empty seed clone/setup before timing; actual insert_slab_batch timed; independent row-to-key posting reconstruction outside timing, output destruction excluded. Low-cardinality8 uses bitmap mode; cardinality100 samples but retains standard lists; disabled control never requests sampling. No runner builds/tests overlap timing; unrelated host activity uncontrolled. No allocation/RSS or full-Db speedup claim from this component.

| Series | Index fixture | Baseline ms | Candidate ms | Median paired time reduction | Positive process pairs |
|---:|---|---:|---:|---:|---:|
| 1 | bitmap_1k | 0.815593 | 0.048073 | 93.955% | 6/6 |
| 1 | bitmap_10k | 8.251604 | 0.475042 | 94.243% | 6/6 |
| 1 | standard_card100 | 1.289219 | 0.069750 | 94.567% | 6/6 |
| 1 | disabled_1k | 0.047906 | 0.045094 | 6.515% | 6/6 |
| 2 | bitmap_1k | 0.790812 | 0.048208 | 93.904% | 6/6 |
| 2 | bitmap_10k | 7.971365 | 0.481135 | 93.986% | 6/6 |
| 2 | standard_card100 | 1.275417 | 0.071135 | 94.501% | 6/6 |
| 2 | disabled_1k | 0.047948 | 0.045312 | 6.165% | 6/6 |

This removes repeated O(rows*100) sample work while key extraction, posting insertion and row predicates retain their original costs. Sample storage exists only during the one-time calculation, not as a persistent cache. Requested bitmap mode is still an explicit IndexDef setting; the ordinary wing/ts benchmark leaves it disabled.

## Full default insertion regression matrix

Fresh compare binaries from exact snapshots (engine-provenance.json), two series ×six alternating process pairs, native and Full durable 1k/10k, 24 samples/case/process, one operation, no warmup. Preparation/setup/readback/drop outside timing, original embedding/FTS/indexes/sync, unchanged SQLite controls. All48 processes complete. These measurements exercise ordinary bitmap-disabled indexes and do not establish a whole-Db bitmap speedup.

| Case | Engine | Series1 paired gain /positive pairs | Series2 paired gain /positive pairs |
|---|---|---:|---:|
| insert_bulk_1k | lin | +0.471% /4/6 | -0.852% /1/6 |
| insert_bulk_1k | sqlite | -1.116% /2/6 | -0.607% /2/6 |
| insert_bulk_10k | lin | -0.196% /3/6 | +0.110% /3/6 |
| insert_bulk_10k | sqlite | -0.160% /3/6 | +0.628% /5/6 |
| durable_insert_1k | lin | +2.218% /4/6 | -0.028% /3/6 |
| durable_insert_1k | sqlite | +3.982% /4/6 | +1.089% /4/6 |
| durable_insert_10k | lin | +0.294% /3/6 | +0.238% /3/6 |
| durable_insert_10k | sqlite | -6.368% /2/6 | +1.492% /4/6 |

Ordinary-path changes are mixed/small; no repeatable default-Db acceleration claim. First durable10k candidate median18.503ms loses SQLite15.728ms; second13.555ms versus13.665ms. Host/process variation is substantial and no universal regression-free inference follows. Durable1k2.289/2.126ms still loses SQLite1.545/1.408ms. Retain every loss and raw outlier. A fresh available-peer matrix is recorded separately, not merged with these protocols.

## Qualification boundary

Any acceptance is limited to explicit bitmap-cardinality sampling. Correctness, component gate and ordinary-path/peer observations are separate evidence. No automatic bitmap policy, RSS claim, physical power-cut, optional-feature or all-platform qualification. Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas objective remains open; durable SQLite and absent live Microsoft endpoints persist. The component speedup is not universal DB superiority.

## Final decision: accepted for sampling-enabled index insertion

The one-time sample removes repeated work with the same choices/errors/postings, and the index-operation improvement repeats in all twelve process pairs. Ordinary insertion results do not establish a general speedup; all noisy cases and SQLite losses remain recorded above.

Final oracle enhancement isolates each boolean-position fixture from other cases (fresh row copies), specifically testing a boolean only at offset100 outside the sample. Prepopulation9999/10000/10001 exercises crossing the dynamic threshold while inserting. Final focused test and full workspace pass. These cfg(test)-only changes happened after timing; original measured candidate-index.rs and final-index.rs are both retained. Fresh final release builds of index worker, compare and peer worker are **byte-identical** to all measured binaries. final-verification.json records final source hashes, final binary hashes and equality, so the test enhancement does not require treating a different executable as measured.

Fresh current-candidate peer refresh: **108/108 aggregate read wins** versus SQLite/DuckDB/PostgreSQL/MySQL/MongoDB/pandas and **15/15 native insertion wins** versus the five database peers, each at1k/10k/100k, three independent processes ×12 samples. Per-case tables and exact contracts in peer-read/report.md and peer-native/report.md; full audit in peer-audit.json. Minimum aggregate speedup1.816x read /1.056x native. Source stable throughout measurement, exact values, positive complete samples, sentinels, no leftover fixture objects and stopped owned servers checked. These finite scenario wins are not universal engine superiority, and native contracts are not cross-engine disk-durability equivalence. The bitmap flag is disabled in this peer fixture.

MSSQL/Kusto configuration remains absent (presence booleans only in endpoint-presence.json); neither counted as a win. Durable1kSQLite gap persists. Full named-peer objective is not complete. No format, sync or public API change, no physical power-cut/RSS/all-platform/optional-feature claim.
