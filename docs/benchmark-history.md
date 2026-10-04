# Полная история бенчмарков Lin

Собрано 2026-10-05. Срез репозитория: `386300e0ba6d69ef87647425acc034c5c5a1b8bd`. Это описание сохранённых измерений и экспериментов проекта и данного диалога; новые измерения при сборке документа не запускались.

Охват: **136 серий**, **158 исходных Markdown-отчётов**, **1514 файлов run.json** (включая три локальных .airbug-bench), **111 уникальных case ID** в сохранённых Airbug-прогонах и **474 сообщений диалога о замерах**. Статусы run.json: complete: 1485, failed: 1, incomplete: 28.

Включены принятые и отклонённые оптимизации, контрольные и повторные прогоны, профили этапов и аллокаций, peer API matrix, ранние версии 0.4.0 и локальные CI/smoke-прогоны. Планы и предварительные сообщения диалога помечены как исторические сообщения, а не самостоятельное доказательство выигрыша. Результаты разных коммитов, машинных состояний, контрактов, native/durable и размеров не объединяются в общий рейтинг. Отсутствующий peer не считается победой.

## Навигация

- [Итог и ограничения](#итог-и-ограничения)
- [Методика и инструменты](#методика-и-инструменты)
- [Каталог серий](#каталог-серий)
- [Описание каждой серии](#описание-каждой-серии)
- [Локальные прогоны](#локальные-прогоны)
- [Все сохранённые case ID](#все-сохранённые-case-id)
- [История сообщений диалога](#история-сообщений-диалога)
- [Реестр исходных прогонов](#реестр-исходных-прогонов)

## Итог и ограничения

Последняя сохранённая матрица чтения 2026-10-04: 108/108 агрегированных доступных сравнений в пользу Lin на 1k/10k/100k, по шести API против SQLite, DuckDB, PostgreSQL, MySQL, MongoDB и pandas. MSSQL и Kusto не настроены. Lin измеряется через prepared Rust API, Python peers — через драйверы/DataFrame с передачей и материализацией. Это не доказательство превосходства во всех нагрузках. [Исходная матрица](../benches/results/2026-10-04-current-peer-read-matrix/report.md).

Последний принятый parallel sparse classifier: durable 10k повторно быстрее SQLite по агрегированным медианам, 5/6 побед в каждой серии; durable 1k проигрывает. Native 1k также остаётся незакрытым случаем. Mongo native имеет отдельные результаты 5.71× на 1k и 4.84× на 10k, с собственным контрактом недолговечной вставки. Общая цель обогнать всех перечисленных peers остаётся непроверенной. Последние mimalloc, half-density normalization и sparse-size reserve отклонены после слабых либо противоречивых результатов; ниже сохранены и первые серии, и повторы.

## Методика и инструменты

### benchmark-contract.md

Источник: [benchmark-contract.md](benchmark-contract.md).

# Benchmark contract

The performance target is a measured win for Lin on explicitly defined workloads
against SQLite, DuckDB, PostgreSQL, MySQL, MongoDB, SQL Server, Kusto, and pandas.
A missing engine, incorrect result, or incompatible workload is not a win.
This is not a claim of superiority for every workload or data size.

## Native Rust comparison

`cargo bench --bench compare` remains the primary comparison for Lin, SQLite,
DuckDB, PostgreSQL and MySQL. Read queries use warm fixtures and prepared APIs.
Single-row update/delete now prepare queries outside timing, validate affected
rows and read-back before/after measurement, and cover 1k/10k/100k rows.
The previous update/delete report included Lin parsing, so it must not be used
as the baseline for this revised execution-only contract.

Airbug's `bench_with_input` and `bench_checked` always exclude input setup and
input destruction. `DropPolicy` controls the returned output's destruction only.
Use `--max-iterations 1 --warmup-ms 0` for expensive fresh-input workloads; the
reported values will then be individual timed operations, with less averaging.

```sh
cargo bench --bench compare -- --filter 1row --samples 8 --max-iterations 1 --warmup-ms 0 --output .airbug-bench/writes
cargo bench --bench compare -- --filter plan_dnf --samples 8 --max-iterations 1 --warmup-ms 0 --output .airbug-bench/planner
```

Existing SQL insert loops are API comparisons. They do not represent optimal
COPY, appender, or batched server ingestion. Lin's embedding and FTS maintenance
also do additional work. Add native bulk ingestion comparisons before claiming
that Lin beats a peer's best ingestion path.

## Extended read API comparison

`scripts/bench-peers.py` adds MongoDB, pandas, SQL Server and Kusto and can also
run SQLite/DuckDB/PostgreSQL/MySQL. `examples/peer_bench.rs` is a persistent Lin
worker, built in release mode. Each engine receives the same deterministic rows.
All results are checked against independently generated expected columns, values,
and multiplicities before/after measurement; row order is not compared.

Cases: point get returning id/title, indexed equality count, substring count,
filtered id/title materialization, inner FK join, and filtered inner FK join.
There is no implicit take limit. Joins use normal inner semantics, not `innerunique`.

Lin uses its prepared Rust row API. Python adapters measure driver/DBAPI and
DataFrame row APIs, including their language dispatch and result materialization.
DuckDB uses PREPARE/EXECUTE; PostgreSQL uses psycopg prepared execution; SQLite
uses its statement cache. PyMySQL/SQL Server/Kusto include their driver's query
submission overhead. These are API measurements, not isolated engine CPU costs.
Server measurements include network round-trip and result transfer. Pandas is
in-memory and makes no durability claim. Kusto query result caching is disabled.

JSON IPC, setup, validation, connection creation, and teardown are outside timers.
Each process/connection repetition is aggregated first, then medians are compared.
Batch-average percentiles are not individual-operation latency percentiles.
Do not measure while compilation, ingestion, or unrelated benchmark work runs.

```sh
python3 -m venv .bench-venv
.bench-venv/bin/python -m pip install -r scripts/bench-peers-requirements.lock
cargo build --release --example peer_bench
.bench-venv/bin/python scripts/bench-peers.py \
  --engines lin sqlite duckdb postgres mysql mongo pandas \
  --rows 10000 --samples 8 --repeats 3 --require-wins \
  --output .airbug-bench/peers-10k
```

Use 100k and 1M datasets, varied selectivity and skew, independent process
repetitions and native ingestion APIs before broadening the conclusion.
`--require-wins` fails if any measured peer beats Lin; unavailable/error cases
always make the run incomplete and return a nonzero exit code.

## Server scope

Only use dedicated benchmark instances/databases. SQL tables and Mongo databases
are uniquely named `linbench_<random>` and only those objects are removed.
Existing application collections/tables are never intentionally reused.

Connection configuration (secrets stay in environment variables):

- `LIN_BENCH_PG_URL`: default local benchmark Postgres on port 55432.
- `LIN_BENCH_MYSQL_URL`: default local benchmark MySQL on port 53306.
- `LIN_BENCH_MONGO_URL`: default local benchmark Mongo on port 27027.
- `LIN_BENCH_MSSQL_HOST`, `PORT` (default 1433), `USER`, `PASSWORD`, `DATABASE`.
- `LIN_BENCH_KUSTO_URL`, `DATABASE`, optional `TOKEN` (bearer access token).

SQL Server and Kusto adapters need live validation against their test instances.
An x86-64 emulator running on an ARM host must be labelled separately and must
not be used to claim a win against native x86-64 or cloud deployments.

CI runs fresh-fixture mutation cases separately with one timed operation per sample.
They are excluded from automatic calibration because untimed 100k-row fixture
construction can otherwise make calibration take excessively long.

## Native MongoDB insertion

Run `scripts/bench-peers.py --engines lin mongo --cases insert_native --rows 1000
--samples 9 --repeats 3 --require-wins --output .airbug-bench/mongo-native` with
`--lin-binary` pointing to the release `peer_bench` example. Native insertion
cannot be mixed with read cases. Unsupported native adapters fail as incomplete.

Each sample uses a fresh fixture. Schema/index creation, preparation, exact
six-field readback, cleanup and Lin IPC are excluded from timing. The timestamp
is shared by both engines within each process. Lin includes default embedding
and FTS. MongoDB uses ordered `insert_many`, maps `id` to `_id`, creates unique
URI and wing/ts indexes, and uses acknowledged writes (`w=1`, `j=false`). This
compares successful native API calls; it does not establish identical bulk
failure atomicity or physical disk durability. Mongo fixtures use unique owned
databases and cleanup only objects successfully created by this run.


### benchmark-results-2026-10-01.md

Источник: [benchmark-results-2026-10-01.md](benchmark-results-2026-10-01.md).

# Benchmark results, 2026-10-01

The goal of beating every peer remains unmet.

Current retention decision: the reciprocal hashing-slot experiment was rejected
and removed after three alternating process pairs. The original modulo path is
retained, together with an independent bitwise embedding regression test.
See `../benches/results/2026-10-01-embed-alternating/report.md` for all pairs.
Its process medians were 18.713 ms for modulo and 22.874 ms for the experiment;
unchanged controls also drifted, so no causal speedup or new peer win is claimed.

## Single-row mutations

Same prepared, read-back-validated harness for before/after. Eight fresh-fixture
samples, one operation per sample, preparation and input destruction excluded.
These short samples are indicative, not stable latency percentiles.

| 100k rows | Before | Final Lin | SQLite in final run |
|---|---:|---:|---:|
| update one row | 72.615 ms | 12.875 us | 15.354 us |
| delete one row | 148.597 ms | 25.563 us | 18.979 us |

Raw evidence: `../benches/results/2026-10-01-write-before-verified/`,
`../benches/results/2026-10-01-write-final/`.
SQLite remains faster for delete and some smaller single-row writes.
Native bulk insertion also remains behind SQLite in the measured loop-based
fixture; optimized native ingestion APIs have not yet been compared.

## Read APIs, 100k rows

Three independent process repetitions, eight samples per case, rotated engine
order. Exact returned values and duplicate multiplicities checked outside timers.
Lin wins 35 of 36 measured peer/case comparisons. DuckDB wins substring count:
Lin 855.437 us versus DuckDB 488.704 us. Lin wins point lookup, equality count,
materialization and both joins against SQLite, DuckDB, PostgreSQL, MySQL,
MongoDB and pandas in this fixture.

See `../benches/results/2026-10-01-peer-100k/report.md` and raw per-process JSON.
This compares Lin's prepared Rust API to Python driver/DataFrame APIs; it does
not isolate engine CPU costs. Server timings include transfer and round-trip.
The native Rust comparison is separately saved under
`../benches/results/2026-10-01-native-core/`.

The earlier 10k peer run used fresh Lin workers/connections, but not independent
Python parent processes. Do not describe it as three independent process runs.

SQL Server and Kusto adapters are implemented but unverified against live test
endpoints. Missing peers are not wins. Next priorities: contiguous text scan
layout, deletion/FTS cost, fair optimized bulk ingestion, then 1M-row scaling,
selectivity/skew variations, and MSSQL/Kusto on supplied test servers.

## Validation

Workspace Rust tests passed with the long posting-delta test excluded on the
latest source. That long test passed earlier during this change; it was not
rerun after the final row-ownership optimization. Six peer-harness contract
tests, formatting and diff whitespace checks passed. CI workflow was edited
and shell syntax checked; remote CI has not been run.

New WAL records use checksummed LIN06 envelopes. Old records remain readable,
but old binaries cannot read new records; writers/followers need coordinated
upgrade. Snapshot/backup formats are unchanged.

## Follow-up: packed title column

`docs | title ~ needle | count` now searches a lazily built contiguous byte
column. Offsets prevent cross-row matches; each hit counts its row once.
Updates, insertion, deletion and rollback invalidate the derived column.
No query results are cached. Empty needles and UTF-8 byte boundaries retain
substring semantics. It adds title-byte storage plus one usize offset per row;
the first count after mutation pays construction cost. Warm benchmark results
must not be described as cold-query latency. Dense-match workloads and cold
construction still need separate performance measurements.

The initial short-needle per-string experiment remains in raw results under
`2026-10-01-substr-short-scan` but is not the retained implementation.
Retained implementation evidence is `2026-10-01-substr-packed`.

Three independent processes confirm substring count at 100k rows: Lin median
209.177 us, DuckDB 485.394 us (2.32x). Exact result count validated in each
process. This closes the previously measured warm substring-count loss, not
the outstanding SQLite write/ingestion losses or missing MSSQL/Kusto coverage.
52 exec regression tests passed, including packed-column boundary/mutation
coverage; the rollback test was then expanded and rerun successfully.

## Follow-up: write allocations and bulk input parity

FTS insert/remove/move tokenization now borrows already lowercase ASCII words
and allocates normalized words only when needed. A sorted, deduplicated vector
replaces the per-row tree of owned token strings. Repeated words across fields,
Unicode case normalization, posting folds, delete and move are regression tested.
Bulk docs map registration reuses the collection map instead of allocating the
collection name and looking it up for every row.

All SQL bulk loop adapters now receive prepared fixture rows outside the timer,
matching the existing Lin prepared input. The earlier native bulk SQL timings
included fixture generation and must not be used for a before/after speedup claim.
The updated run `2026-10-01-bulk-borrowed-tokens` still loses to SQLite:
Lin 14.216 ms versus SQLite 10.645 ms for 10k rows. DuckDB there uses row inserts,
not its optimized appender; that result is not proof of superior native ingestion.

Phase diagnostics (`2026-10-01-insert-phases`, eight samples, one process) show
full Lin insertion at 14.484 ms, without embedding 9.718 ms, additionally without
secondary index 8.428 ms, additionally without FTS 6.278 ms. These are diagnostic
variants, not interchangeable correctness-equivalent winners. The retained full
configuration still performs embedding, FTS and scalar indexing.

Workspace tests excluding the long posting-delta persistence stress test passed
on the updated source. That long test has not yet been rerun for borrowed tokens.

## Follow-up: native ingestion competitor

Added DuckDB Appender benchmarks at 1k/10k. Flush is timed; sampled readback
compares all six columns and row multiplicities with the exact input fixture.
The common run `2026-10-01-bulk-with-appender` shows a native ingestion loss
too: Lin full docs ~18.7 ms, SQLite ~14.4 ms, DuckDB Appender ~13.7 ms at 10k.
Eight samples in one process are not independent process replication; host
load and CPU frequency are uncontrolled. Earlier isolated appender timing was
~22 ms, so do not mix unrelated runs to claim Lin wins native ingestion.

An experiment eliminating the moved-row copy during delete did not yield a
convincing gain and was removed. The retained writer uses the earlier tested
swap-remove implementation. Its experiment is saved as `2026-10-01-write-owned-move`.

The posting-fold stress fixture now uses 20 bulk seed commits instead of 10k
unrelated durable single-row seed commits. Its 900 individual deletes, survivor
assertions, checkpoint, reopen and no-resurrection checks are retained.

The retained source passed all 52 exec tests after removing the delete-copy
experiment. The complete posting-fold persistence stress test also passed
(20.32 s), including the 900 deletes and checkpoint/reopen assertions.
Formatting and diff whitespace checks passed. Native appender benchmark
compiled, ran and passed sampled exact readback validation.

## Follow-up: exact hashing-slot experiment

On 64-bit targets, the hashing accumulator now experimentally replaces repeated
runtime division with a precomputed floor reciprocal, high-half multiplication
and one correction subtraction. Hashes, slot indices and float accumulation
order are unchanged. Other pointer widths retain modulo. Extreme/random integer
tests and an independent dense reference verify exact modulo and embedding
float bit patterns across several dimensions, ASCII, Unicode and repeated words.

Performance is NOT validated: `2026-10-01-embed-before` and
`2026-10-01-embed-reciprocal` ran under substantial unrelated CPU load.
Process inspection found multiple rustc processes, a CAD test and virtualization
consuming cores. Unchanged no-embedding controls drifted substantially. Their
raw reports are annotated with measurement-limitations.json. Do not claim a
speedup or any peer win from those reports. Retention requires a controlled
repeat; the goal remains unmet and this experiment remains provisional.

Current reciprocal experiment source passed the complete workspace test suite
without excluding the posting-fold persistence stress test (24.44 s for the
readback suite). All six peer-harness contract tests, format and diff whitespace
checks also passed. This proves regression coverage, not a performance win.

## Retention decision after alternating repeats

Completed three independent process pairs in alternating order. Both variant
sources and SHA-256 hashes are saved with the results. The reciprocal variant
did not consistently beat modulo and was removed. The older section describing
a provisional reciprocal implementation is superseded by this decision.
An independent dense modulo reference test remains to protect exact vector bits
for varied dimensions, case normalization, Unicode and repeated words.

Retained modulo source passed both embedding unit tests and all four embedding
integration tests after removal of the experiment. Formatting and whitespace
checks passed. These checks validate vector compatibility, not the full goal.

## Exact sparse normalization and pack allocation follow-up

Sparse embedding normalization now avoids sorting touched slots when the squared
sum is below 2^20. Built-in weights are positive multiples of 1/4, so the sum
is exact on the 1/16 grid in this range. At and above the boundary, or for a
non-finite sum, the original sorted accumulation is retained. Dense-reference
float-bit tests cover the boundary and randomized larger values.
Three alternating independent process pairs in `2026-10-01-norm-alternating`
all improved full 10k insertion; median process results were 14.131 ms sorted
versus 13.131 ms exact-sum (about 7%). This is a local optimization result,
not proof of beating every peer.

The pack now tracks written row identities only when a statement requires
`cas each`; mixed packs still track all preceding writes. Embedding text reserves
its final capacity once, preserving empty-field separators. Both changes passed
the complete workspace suite before the following measurement.

`2026-10-01-bulk-pack-tracking` measured eight fresh fixtures per case in one
process: full Lin 10k insertion 12.536 ms, SQLite 10.425 ms, DuckDB Appender
10.438 ms. At 1k: Lin 1.196 ms, SQLite 0.841 ms, Appender 1.231 ms.
Fixtures and SQL statement preparation are outside timing; appender flush is
inside timing and exact readback passed. Lin includes default embedding and FTS;
SQL peers use the plain fixture schema. Background CPU load is uncontrolled.
This single run cannot establish the causal effect of the allocation changes.
The 10k ingestion goal remains unmet; MSSQL and Kusto remain unverified.

## FTS cancellation correctness

A new regression reproduced a stale candidate after repeatedly removing and
reinserting a term at the same row position. A pending addition may overlap the
base posting after cancelling a removal. Cancelling that addition must still
record a deletion, otherwise the old base entry becomes visible again.
The fix retains the no-base-search delete path. The regression exercises 600
remove/reinsert cycles and intervening folds. Benchmark results above predate
this correctness fix and must not be presented as measurements of its cost.

After the FTS fix, `cargo test --workspace --offline` passed, including all
16 library tests, 52 exec tests and the durable posting-fold stress suite.

## Borrowed embedding text

Single effective title/body/snippet fields now borrow their stored text instead
of allocating a copy; multi-field concatenation and trailing empty-field spaces
are preserved. Unit coverage checks borrowed storage and vector equivalence.
The workspace suite passed before a helper-only rename; embedding unit tests
were rerun on the final helper. Diagnostic phase results are saved in
`2026-10-01-insert-borrowed-text`. They have no paired baseline and were collected
under uncontrolled load; no speedup or peer win is claimed from this run.

## Rejected zero-marker accumulator experiment

Removing the embedding accumulator's `seen` bitmap preserved vector tests but
failed the repeated performance comparison: full 10k insertion 13.178 ms original
versus 13.285 ms candidate. The earlier apparent gain had large unchanged-control
drift. The candidate was removed; reports are retained in `embed-seen-*` directories.
No speedup or progress toward universal peer wins is claimed from this experiment.

## Rejected bulk row construction

Replacing individual map inserts with iterator collection regressed all four
10k insertion phase cases: full 12.480 to 13.371 ms, stripped 5.691 to 6.441 ms.
It was removed. Exact duplicate-field and execution-time regression coverage
remains. Raw reports and comparison are in `2026-10-01-row-build-before` and
`2026-10-01-row-build-after`. No peer win was established.

## Profile-directed prepared-template experiment

macOS sample captured a five-second stack profile of full insertion. It includes
fixture preparation as well as execution, so counts are not isolated engine CPU
percentages. The profile identifies record construction/allocation within the
execution stack as a candidate. Raw profile: `2026-10-01-insert-profile.txt`.
Its 400-observation benchmark was profiled and must not be used as an uninstrumented
performance comparison.

Prepared static row templates passed workspace tests but were rejected after
three alternating independent process pairs: median full 10k insertion 13.320 ms
original vs 13.371 ms templates. Additional retained memory was not justified.
Both source variants, hashes, raw runs and decision are saved under
`2026-10-01-insert-templates-ab`. The original implementation was restored.
No peer win was established, and the full goal remains unmet.

## Rejected sorted scalar-index bulk build

An empty-tree bulk construction path sorted keys, grouped duplicate postings,
and built the BTreeMap in one pass. Existing trees retained incremental inserts.
Workspace tests and direct index parity/uniqueness checks passed. Three alternating
independent process pairs rejected the performance hypothesis: full insertion
process-median aggregate 12.893 ms original vs 13.826 ms sorted, with the candidate
losing each pair. No-embedding aggregate also regressed (9.094 vs 9.871 ms).
The candidate was removed. Sources, hashes, raw runs and decision are in
`2026-10-01-bulk-tree-ab`. No peer win was established.

## Retained FTS append path

Fresh slabs without pending FTS edits now append directly. This avoids sorting
and deduplicating per-row tokens, since monotonic positions permit duplicate
suppression at posting-list tails. Pending deltas retain the general path to
handle recycled tail positions. Field iteration also avoids a temporary vector.
The workspace suite passed; a targeted test compares general insertion with
fast append across Unicode, repeated tokens, tail recycling and folds.
Three alternating process pairs all improved full 10k insertion (3.2%, 8.5%,
4.5%; median paired improvement 4.5%). The change is retained. This is local
optimization evidence under uncontrolled background load, not proof of winning
against every peer. Sources, hashes and raw runs: `2026-10-01-fts-append-ab`.

Current peer insertion rerun (`2026-10-01-bulk-fts-append`, one process, 12 fresh
fixtures): 10k Lin 14.636 ms, SQLite 10.621 ms, DuckDB Appender 10.480 ms.
At 1k: Lin 1.096 ms, SQLite 0.781 ms, Appender 1.232 ms. Exact appender readback
passed. Default Lin embedding/FTS and plain SQL schemas remain a work difference.
These results still contradict achieving the 10k ingestion goal; they do not
replace the alternating baseline evidence for the local FTS improvement.

## Current complete available-peer read rerun

`2026-10-01-peer-current-100k`: 100k rows, three independent Python processes,
rotated engine order, nine samples. Exact values/multiplicities validated outside
timing. Lin wins all 36 measured aggregate read comparisons with SQLite, DuckDB,
PostgreSQL, MySQL, MongoDB and pandas. MSSQL/Kusto are missing in each process;
12 of the full 48 read comparisons remain unverified and the command exits 2.
This is Rust prepared API versus Python driver/DataFrame APIs, including server
round-trip/transfer. It is not an isolated engine CPU comparison. Current ingestion
losses remain; the overall goal is not achieved. Raw runs, full report, summary,
source hashes and explicit completion audit are saved in this result directory.

## Rejected owned deletion result

An experiment returned the removed owned row rather than cloning the result,
while retaining a clone for rollback. Workspace tests and a new multi-row order/
rollback regression passed. Three alternating process pairs with 40 fresh
fixtures per case did not establish a consistent gain. Pair 3 regressed all
sizes; 100k Lin 23.480 to 27.041 µs while the SQLite control improved slightly.
The candidate was removed; the new behavior regression remains. Raw source
variants/hashes, all sizes, controls and decision are in
`2026-10-01-delete-owned-result-ab`. No point-delete peer win was established.

## Rejected per-batch byte-pair slot cache

A 256 KiB per-batch lazy table reused hashed bigram slots for at least 1024 texts.
All 65536 byte pairs, varied dimensions, reuse and a Unicode batch matched exact
float bits; workspace tests passed. Three alternating independent process pairs
rejected retention: full 10k original/cached 24.435/30.080 ms, 12.257/15.419 ms,
14.320/16.193 ms. Background CPU load was substantial; unchanged controls drifted.
No causal regression percentage or peer win is claimed. The cache was removed;
its extra allocation is not justified by the evidence. Sources, hashes, raw runs
and decision: `2026-10-01-pair-slots-ab`. The full goal remains unmet.

## Prepared cell values and large-insert correctness

The more compact prepared-cell experiment passed workspace and synthetic time/
duplicate-field tests but was removed after inconsistent paired results: full
10k 12.310/15.948 ms, 12.499/12.399 ms, 12.305/11.595 ms (original/cached).
Sources, hashes and raw runs are in `2026-10-01-prepared-cells-ab`.

A separate regression found that >128-row inserts elide Handle.rows, and the WAL
was incorrectly built from that empty result. Exporting a 129-document insertion
reproduced a replica with zero documents. WAL now reads the actual inserted store
slab; affected counts derive from the final insertion statement when results are
elided. Written-row tracking also uses the actual inserted slab. The empty large
Handle remains an optimization. Memory count tests cover 1/128/129/256 rows; a WAL
test compares all 129 rows, fields and embeddings in a fresh replica and verifies
checkpoint/reopen. The complete workspace suite passed after the fix.

Native Lin bulk benchmarks now check affected count and validate every inserted
row's static fields, timestamp type and embedding outside timing. Older reports
are historical: their in-memory operation did insert rows, but those phase/bulk
runs did not prove large-WAL safety or correct affected counts. No new speedup
or full-goal completion is claimed from this correctness fix.

After the correctness fix, the strengthened native bulk benchmark passed.
10k medians: Lin 12.762 ms, SQLite 10.585 ms,
DuckDB Appender 10.205 ms. One process/eight observations;
background load uncontrolled. Correct affected counts and Lin readback were
validated outside timing. The 10k ingestion losses remain. Report:
`2026-10-01-bulk-wal-count-fixed`.

## Rejected parallel embedding

Fresh scoped threads did not consistently improve full 10k insertions across
three process pairs (`2026-10-01-parallel-embed-ab`). A reusable Rayon pool also
failed to demonstrate a consistent win (`2026-10-01-embed-pool-ab`); controls were
strongly unstable. Both candidates and the direct Rayon dependency were removed.
Warm-pool wall time does not establish cold-start latency or equal CPU cost.
The serial sparse normalization and FTS append fast path remain.

Final serial native bulk verification (one process, 12 observations):
1k Lin 1.759 ms, SQLite 1.110 ms,
DuckDB Appender 1.501 ms;
10k Lin 15.738 ms, SQLite 13.927 ms,
DuckDB Appender 12.193 ms.
Readback validators passed. Ingestion goal remains unmet. Raw observations,
source hashes and report: `2026-10-01-serial-final-bulk`.

## Retained short index key and bound storage

Private scalar keys use two inline parts, spilling longer composite keys. Search
prefixes and bounds also stay inline. Six alternating prebuilt-binary process
pairs show 3.4% median paired full-10k insertion improvement (4/6 wins), 3.7%
without embedding, while the no-scalar-index control was 3.1% slower. Initial
three build-between-run pairs also won, but are weaker evidence. Final equality
search improved 10.6% across six pairs (6/6 wins); range improved 4.3% (5/6 wins).
These are local noisy measurements, not universal percentages. Complete workspace
tests passed, including new short/spilled ordering and reverse-key mutation tests.
Native ingestion still loses SQLite and DuckDB Appender; overall peer goal is
not complete. Full tables, source hashes and observations:
`2026-10-01-inline-index-ab/report.md`.

## Rejected reverse-map reserve and position move

Reverse-map bulk reservation won only 2/6 full-insert prebuilt process pairs and
was reverted (`2026-10-01-index-reserve-ab`). Scalar key position reuse on delete
passed the workspace suite and readback checks, but 1k lost all three pairs and
100k lost two of three; 10k won three. It was also reverted
(`2026-10-01-index-move-ab`). Tables and raw observations are retained. No new
peer win or causal regression percentage is claimed. Source hashes exactly
match the retained inline-key/bound state after restoration.

## Rejected identity-map position reuse

Keeping moved-row ID/URI/SPO keys and updating only their positions passed all
workspace tests and readback validators. Three alternating prebuilt process
pairs, 24 samples per case: 1k and 100k each won 2/3; 10k lost 3/3. SQLite still
won every matched native deletion measurement. Control timing also fluctuated.
No causal percentage is claimed. Candidate reverted, retained source hashes
verified exactly. Full table and raw runs: `2026-10-01-map-position-ab`.

## Prepared write diagnostic profile

Added `examples/profile_writes.rs` to isolate precompiled delete/reinsert churn
from seed/prepare. A five-second macOS sample of 10000-row churn exposed frequent
FTS union/subtract and move/remove functions. The 25-second run completed and
validated all final ID/URI lookups. This is not the fresh-fixture native benchmark
and establishes no peer win. Raw sample, limitations and source hashes:
`2026-10-01-prepared-write-profile/report.md`. Next investigation: FTS delta
folding/posting work, with native mutation A/B required for retention.

## FTS pending-order correctness correction

Pending merge inputs were assumed sorted but push/cancel did not preserve order.
The original code retained position 2 after deleting 8 then 2 from positions 0..9.
Sorted unique insertion and order-preserving cancellation fix this. Regression
failed before and passed after; descending removals, 3000 oracle-checked random
edits, fold and codec roundtrip also passed. Workspace suite passed after the
production fix. Fused merge was deferred, so no speedup is claimed. Historical
churn profiles did not prove arbitrary-order FTS correctness. Current native
delete validation and source hashes: `2026-10-01-fts-sorted-pending/report.md`.
The full peer goal remains unfulfilled.

## Rejected in-place FTS fold

Measured against the corrected sorted-pending baseline, not the historical
incorrect code. Three alternating independent process pairs on prebuilt binaries,
12 seconds each, 10000-row churn: two losses and one win. Candidate reverted.
Workspace tests and exact post-run lexical IDs/multiplicity checks passed. The
new lexical validation remains in the diagnostic example. Churn rate is not
fresh-fixture native deletion latency or a peer win. Sources and raw results:
`2026-10-01-fts-inplace-fold-ab/report.md`.

## Retained fused sorted FTS merge

After the pending-order correctness fix, folding and pending-result lookup use
one merged traversal/output allocation instead of an intermediate kept list.
All 32768 small-set combinations match the two-pass reference. Workspace tests
and exact diagnostic lexical IDs/multiplicity checks passed. Three prebuilt
independent process pairs improve churn throughput by 12.51%, 0.06%,
21.91% (median 12.51%); the second is nearly tied.
Background load is uncontrolled. Current native deletion and 10k ingestion still
lose; bulk append cases do not exercise these merges, so no bulk speedup is
attributed to this patch. Full tables and hashes:
`2026-10-01-fts-fused-correct-ab/report.md`. Overall goal remains unfulfilled.

## Retained ASCII lowercase path

Prepared-bulk sampling exposed Unicode character conversion calls even for ASCII
fixture text. Lowercase ASCII now borrows after byte classification; uppercase
ASCII reuses scratch with make_ascii_lowercase; Unicode mapping is unchanged.
All 128 ASCII characters, mixed Unicode, scratch reuse, borrowed identity,
embedding-bit checks and complete workspace tests passed. Three prebuilt process
pairs won full 10k insertion with 6.8%, 9.0%, 7.6% reductions (median 7.6%), but
control cases also improved; no full causal percentage is claimed. Current 10k
Lin/SQLite/Appender medians 11.666/10.856/10.409 ms still leave ingestion losses.
Full source/build provenance and observations: `2026-10-01-ascii-lower-ab/report.md`.

## Shared immutable identity-map keys

Private ID/URI maps share cell Arc<str> data; Row and persistence formats stay
unchanged. Frozen-source prebuilt comparisons avoid concurrent live exec edits.
Six pairs: full 10k insertion median paired reduction 8.0% (4/6 wins), point
reads effectively tied (3/6 wins). Matched single writes remain mixed; 100k update
loses two of three baseline pairs. The change is retained for ingestion/memory
as partial work, with the single-row tradeoff explicitly unresolved. URI-only
alternative won 1/3 insertion pairs and was not selected. Workspace tests passed.
No all-peer victory or goal completion is claimed. Raw writes, URI alternative,
source snapshot, hashes and native peer timings:
`2026-10-01-shared-identity-ab/report.md`.


### Исходные команды и средства измерения

- [benches/compare.rs](../benches/compare.rs)
- [examples/peer_bench.rs](../examples/peer_bench.rs)
- [scripts/bench-peers.py](../scripts/bench-peers.py)
- [scripts/test-bench-peers.py](../scripts/test-bench-peers.py)
- [scripts/ci-bench.sh](../scripts/ci-bench.sh)
- [scripts/check-bench-budget.py](../scripts/check-bench-budget.py)
- [benches/ci-baseline.json](../benches/ci-baseline.json)
- [docker-compose.bench.yml](../docker-compose.bench.yml)
- [.github/workflows/bench.yml](../.github/workflows/bench.yml)

### Историческое описание benchmark suite из README

## Бенчмарки (airbug-bench): Lin vs SQLite vs DuckDB vs Postgres vs MySQL

Сравнительные hot paths в `benches/compare.rs` через [airbug](https://github.com/themoretheless/airbug) (`airbug` + `airbug-bench` @ git `release`, `Suite`, `harness = false`).

```bash
# список кейсов
cargo bench --bench compare -- --list

# быстрый прогон (нужен --release; airbug-bench отказывается от debug)
cargo bench --bench compare -- --profile quick

# как в CI: без durable/cold/wal + markdown + .airbug-bench/ci/
# после прогона печатает ссылку на airbug dash (http://127.0.0.1:8790/)
./scripts/ci-bench.sh

# только point get / insert / join / append_log / FTS / фазовый профиль
cargo bench --bench compare -- --filter point_get
cargo bench --bench compare -- --filter insert_bulk_1k --samples 8
cargo bench --bench compare -- --filter join
cargo bench --bench compare -- --profile quick --filter append_log
cargo bench --bench compare -- --profile thorough --tag fts
cargo bench --bench compare -- --profile thorough --tag phase
```

### CI

Workflow [`.github/workflows/bench.yml`](../.github/workflows/bench.yml) на `push`/`pull_request` → `main`: `--profile quick`, без Docker. В отчёте Lin / SQLite / DuckDB / HashMap; Postgres и MySQL пропускаются без серверов. FTS query cases включены; диагностические `*phase*` и шумные durable/cold/wal/reopen cases исключены. После прогона `scripts/check-bench-budget.py` сравнивает Lin median с [`benches/ci-baseline.json`](../benches/ci-baseline.json): **warn** при >1.5×, **fail** при >3× на `point_get` / `filter_eq` / `join_inner`. Иначе job падает только при ошибке compile/harness.

Где смотреть: **Actions → bench → Job summary** (markdown-таблица) и artifact **`bench-report`** (`run.json` + `report.html` + `bench-report.md`). Локально: `./scripts/ci-bench.sh` сразу печатает (и при живом hub открывает) **airbug dash** `http://127.0.0.1:8790/`, затем гоняет бенчи в `.airbug-bench/ci/` (hub: `cargo run -p airbug-hub -- serve --root <lin>`).

Движки: **Lin**, **SQLite** (`rusqlite` bundled), **DuckDB** (bundled; собирается на mac aarch64), **Postgres** / **MySQL** (опционально, через URL), плюс **HashMap** только для point get. N=10 000 для тёплых чтений (fixture; setup вне тайминга). Bulk insert: схема/индекс в setup, в тайминге только запись.

Postgres bulk insert проверяет все шесть записанных полей отдельным подключением после commit, вне таймера. Помимо построчного INSERT в транзакции есть `insert_native_1k/postgres_copy` и `insert_native_10k/postgres_copy`: binary COPY с теми же индексами, проверкой числа строк и полным readback.

MySQL native fixtures используют уникальные обычные InnoDB-таблицы в выбранной БД; общий `docs`/`users`/`orders` не переиспользуется. Cleanup удаляет только таблицы конкретной fixture. Нужны CREATE/DROP TABLE privileges. `insert_native_1k/mysql_batch` и `insert_native_10k/mysql_batch` выполняют один подготовленный multi-row INSERT в транзакции: подготовка вне таймера, создание параметров, запись и commit внутри. Native и построчные bulk cases проверяют все шесть полей отдельным соединением после измерения.

Для проверки затрат нормализации FTS на bulk insert можно задать `LIN_BENCH_MIXED_CASE=1` и выбрать `--filter insert_bulk`: заголовки fixture будут со смешанным регистром у всех движков. Этот режим предназначен для вставок; обычные read cases используют условия для стандартных строчных заголовков.

Сравнимо: point get по id, `wing ==`, range `wing`+`ts`, substring (`title ~ "wal" | count` ≈ `COUNT(*) … LIKE '%wal%'`), materialize `SELECT id,title`, **join** (10k `orders` ⋈ 1k `users` по FK; Lin `run_batch` / SoA+`RecordBatch` и lazy cursor vs SQL `INNER JOIN`; плюс `total > 100` затем join), bulk insert 1k/10k, **append_log** (`append facts` vs `INSERT INTO logs`) 1k/10k.
Join: row-API (`run` → `Vec<Row>`) и OLAP-путь (`run_batch` → `RecordBatch`) рядом; бенч join меряет batch. DuckDB — референс columnar OLAP.  

Lin-only diagnostics:
- **FTS:** `fts_lex_selective`, `fts_lex_common`, `fts_lex_miss`, `fts_hybrid_common`, плюс `reopen_phase_5k/rebuild_fts`. Lex `take` использует bounded top-k heap и клонирует только результат; SQL `LIKE` остаётся отдельным сравнительным shape.
- **Insert phases:** обычный insert, `lin_no_embed`, `lin_no_embed_no_scalar_index` и `lin_no_embed_no_scalar_no_fts`; последовательные разницы оценивают цену hashing embedder, scalar-index и FTS maintenance.
- **Cursor phases:** `lin_cursor_open`, `lin_cursor_scan_project`, `lin_cursor_scan_projected_row`, обычный и compact lazy join cursor. Разницы отделяют setup, `Row=BTreeMap` materialization и join probe/emission.
- **Reopen phases:** полный hot/cold open рядом с отдельными rebuild row maps / scalar indexes / FTS на 5k строк. `Stats::reopen` даёт real-open breakdown: setup/lock, snapshot decode, WAL replay, metadata/head, indexes, row maps и FTS.
- **Group commit:** `group_commit_16/lin_sequential_full` против `lin_grouped_full` — одинаковые 16 уникальных append statements, 16 flush против одного.

Не сравниваем здесь (и не подтасовываем): Lin `hop`/`match`, neural vec, CAS — отдельный слой. Фазовые цифры являются диагностическими разностями медиан, не additive tracing: их нельзя механически суммировать из-за cache state и allocator noise.

Таргетированный `thorough` прогон 2026-09-17 (один процесс, локальная машина): FTS common top-20 `568 → 182 µs`; insert 10k full `27.38 → 20.47 ms`; compact scan/project `1.68 → 0.90 ms`; compact SoA join cursor `1.70 → 0.48 ms`; hot reopen 5k `158.6 → 82.0 ms`; 16 уникальных Full commits `5.05 ms` sequential против `0.83 ms` grouped. Это benchmark evidence, не переносимый SLA.

### Расширенное сравнение и контракт

[`docs/benchmark-contract.md`](benchmark-contract.md) фиксирует условия измерений.
`scripts/bench-peers.py` сравнивает проверяемые результаты Lin с SQLite, DuckDB,
PostgreSQL, MySQL, MongoDB, SQL Server, Kusto и pandas. Недоступные движки дают
неполный отчёт и ненулевой exit code; `--require-wins` дополнительно проверяет
победу Lin в каждом измеренном сценарии. Rust row API и Python driver/DataFrame
API измеряются отдельно от существующего native Rust harness.

```sh
python3 -m venv .bench-venv
.bench-venv/bin/python -m pip install -r scripts/bench-peers-requirements.lock
cargo build --release --example peer_bench
.bench-venv/bin/python scripts/bench-peers.py --engines lin sqlite duckdb postgres mysql mongo pandas --repeats 3 --require-wins --output .airbug-bench/peers
```

Update/delete: подготовка вне таймера, read-back validation, 1k/10k/100k строк.
`--max-iterations 1 --warmup-ms 0` ограничивает дорогие fresh-input прогоны.

### Postgres / MySQL

Postgres fixtures создают отдельную схему `lin_bench_*` для каждого подключения и удаляют её при завершении. Таблицы остаются обычными WAL-logged таблицами; schema setup и cleanup находятся вне таймера. Пользователю подключения нужен `CREATE` на тестовой базе. Для отдельного прогона PostgreSQL установите `LIN_BENCH_SKIP_MYSQL=1`: это отключает подключение и setup MySQL.

Без сервера кейсы пропускаются (Lin/SQLite/DuckDB всё равно бегут). URL: `LIN_BENCH_PG_URL` / `LIN_BENCH_MYSQL_URL`, иначе авто-probe локальных портов.

```bash
# рекомендуемый стек (в корне lin)
docker compose -f docker-compose.bench.yml up -d
# либо явно:
export LIN_BENCH_PG_URL='postgresql://lin:lin@127.0.0.1:55432/lin'
export LIN_BENCH_MYSQL_URL='mysql://lin:lin@127.0.0.1:53306/lin'
```

Альтернативы из `Documents/Sources`:
- MySQL smoke из **dbill**: `docker compose -f ../dbill/docker-compose.yml up -d mysql` → `mysql://dbill:dbill@127.0.0.1:33306/dbill_smoke`
- Postgres через Homebrew: задача **ppduster** `macos-stack-postgres` (`postgresql@17`); после старта сервиса probe `postgresql://postgres@127.0.0.1:5432/postgres`

Dev-deps: `airbug` (unit) и `airbug-bench` (бенчи) с ветки `release`; плюс `postgres` / `mysql`.


## Каталог серий

| Серия | Описания | run.json | Прочие JSON | HTML | Логи |
|---|---:|---:|---:|---:|---:|
| [0.4.0-core-phases](../benches/results/0.4.0-core-phases) | 0 | 1 | 2 | 1 | 0 |
| [0.4.0-fts-phases](../benches/results/0.4.0-fts-phases) | 0 | 1 | 2 | 1 | 0 |
| [0.4.0-full-70](../benches/results/0.4.0-full-70) | 0 | 1 | 2 | 1 | 0 |
| [0.4.0-insert-phases-repeat](../benches/results/0.4.0-insert-phases-repeat) | 0 | 1 | 2 | 1 | 0 |
| [0.4.0-join-phases-repeat](../benches/results/0.4.0-join-phases-repeat) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-absolute-timestamps](../benches/results/2026-10-01-absolute-timestamps) | 1 | 1 | 6 | 1 | 5 |
| [2026-10-01-ascii-lower-ab](../benches/results/2026-10-01-ascii-lower-ab) | 1 | 7 | 27 | 7 | 11 |
| [2026-10-01-borrowed-delete-ab](../benches/results/2026-10-01-borrowed-delete-ab) | 1 | 6 | 16 | 6 | 13 |
| [2026-10-01-bulk-borrowed-tokens](../benches/results/2026-10-01-bulk-borrowed-tokens) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-exact-norm](../benches/results/2026-10-01-bulk-exact-norm) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-fts-append](../benches/results/2026-10-01-bulk-fts-append) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-pack-tracking](../benches/results/2026-10-01-bulk-pack-tracking) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-tree-ab](../benches/results/2026-10-01-bulk-tree-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-bulk-tree-before](../benches/results/2026-10-01-bulk-tree-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-wal-count-fixed](../benches/results/2026-10-01-bulk-wal-count-fixed) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-bulk-with-appender](../benches/results/2026-10-01-bulk-with-appender) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-committed-local-read-audit](../benches/results/2026-10-01-committed-local-read-audit) | 4 | 4 | 1 | 0 | 2 |
| [2026-10-01-delete-owned-result-ab](../benches/results/2026-10-01-delete-owned-result-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-delete-owned-result-after](../benches/results/2026-10-01-delete-owned-result-after) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-delete-owned-result-before](../benches/results/2026-10-01-delete-owned-result-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-dense-reverse-ab](../benches/results/2026-10-01-dense-reverse-ab) | 1 | 18 | 40 | 18 | 22 |
| [2026-10-01-doc-slab-scan](../benches/results/2026-10-01-doc-slab-scan) | 1 | 24 | 51 | 24 | 27 |
| [2026-10-01-duckdb-appender](../benches/results/2026-10-01-duckdb-appender) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-durable-profile](../benches/results/2026-10-01-durable-profile) | 1 | 12 | 26 | 12 | 16 |
| [2026-10-01-embed-alternating](../benches/results/2026-10-01-embed-alternating) | 1 | 6 | 13 | 6 | 0 |
| [2026-10-01-embed-before](../benches/results/2026-10-01-embed-before) | 0 | 1 | 3 | 1 | 0 |
| [2026-10-01-embed-default-modulo](../benches/results/2026-10-01-embed-default-modulo) | 1 | 12 | 26 | 12 | 14 |
| [2026-10-01-embed-pool-ab](../benches/results/2026-10-01-embed-pool-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-embed-reciprocal](../benches/results/2026-10-01-embed-reciprocal) | 0 | 1 | 3 | 1 | 0 |
| [2026-10-01-embed-seen-after](../benches/results/2026-10-01-embed-seen-after) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-embed-seen-after-repeat](../benches/results/2026-10-01-embed-seen-after-repeat) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-embed-seen-before](../benches/results/2026-10-01-embed-seen-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-embed-seen-before-repeat](../benches/results/2026-10-01-embed-seen-before-repeat) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-embed-sparse-output](../benches/results/2026-10-01-embed-sparse-output) | 1 | 12 | 26 | 12 | 14 |
| [2026-10-01-empty-store-uniqueness](../benches/results/2026-10-01-empty-store-uniqueness) | 1 | 12 | 26 | 12 | 16 |
| [2026-10-01-fts-append-ab](../benches/results/2026-10-01-fts-append-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-fts-append-before](../benches/results/2026-10-01-fts-append-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-fts-entry-ref](../benches/results/2026-10-01-fts-entry-ref) | 1 | 24 | 51 | 24 | 26 |
| [2026-10-01-fts-fused-correct-ab](../benches/results/2026-10-01-fts-fused-correct-ab) | 1 | 2 | 9 | 2 | 9 |
| [2026-10-01-fts-inplace-fold-ab](../benches/results/2026-10-01-fts-inplace-fold-ab) | 1 | 0 | 6 | 0 | 8 |
| [2026-10-01-fts-slab-scratch](../benches/results/2026-10-01-fts-slab-scratch) | 1 | 24 | 51 | 24 | 28 |
| [2026-10-01-fts-sorted-pending](../benches/results/2026-10-01-fts-sorted-pending) | 1 | 1 | 3 | 1 | 0 |
| [2026-10-01-index-move-ab](../benches/results/2026-10-01-index-move-ab) | 1 | 6 | 16 | 6 | 7 |
| [2026-10-01-index-reserve-ab](../benches/results/2026-10-01-index-reserve-ab) | 2 | 12 | 28 | 12 | 13 |
| [2026-10-01-index-same-key](../benches/results/2026-10-01-index-same-key) | 1 | 6 | 14 | 6 | 10 |
| [2026-10-01-index-visit-ab](../benches/results/2026-10-01-index-visit-ab) | 2 | 12 | 29 | 12 | 16 |
| [2026-10-01-inline-index-ab](../benches/results/2026-10-01-inline-index-ab) | 1 | 37 | 86 | 37 | 40 |
| [2026-10-01-insert-borrowed-text](../benches/results/2026-10-01-insert-borrowed-text) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-insert-phases](../benches/results/2026-10-01-insert-phases) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-insert-profile](../benches/results/2026-10-01-insert-profile) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-insert-templates-ab](../benches/results/2026-10-01-insert-templates-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-map-position-ab](../benches/results/2026-10-01-map-position-ab) | 1 | 6 | 16 | 6 | 7 |
| [2026-10-01-merge-proposal](../benches/results/2026-10-01-merge-proposal) | 1 | 12 | 27 | 12 | 15 |
| [2026-10-01-native-after-wal](../benches/results/2026-10-01-native-after-wal) | 1 | 3 | 8 | 3 | 3 |
| [2026-10-01-native-core](../benches/results/2026-10-01-native-core) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-native-profile-current](../benches/results/2026-10-01-native-profile-current) | 1 | 0 | 1 | 0 | 2 |
| [2026-10-01-norm-alternating](../benches/results/2026-10-01-norm-alternating) | 1 | 6 | 13 | 6 | 0 |
| [2026-10-01-pair-slots-ab](../benches/results/2026-10-01-pair-slots-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-pair-slots-before](../benches/results/2026-10-01-pair-slots-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-parallel-embed-ab](../benches/results/2026-10-01-parallel-embed-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-parallel-embed-before](../benches/results/2026-10-01-parallel-embed-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-peer-100k](../benches/results/2026-10-01-peer-100k) | 4 | 4 | 0 | 0 | 0 |
| [2026-10-01-peer-10k](../benches/results/2026-10-01-peer-10k) | 1 | 1 | 0 | 0 | 0 |
| [2026-10-01-peer-current-100k](../benches/results/2026-10-01-peer-current-100k) | 5 | 4 | 1 | 0 | 0 |
| [2026-10-01-pg-owned-schema](../benches/results/2026-10-01-pg-owned-schema) | 1 | 1 | 4 | 1 | 3 |
| [2026-10-01-pg-validated-insert](../benches/results/2026-10-01-pg-validated-insert) | 1 | 3 | 10 | 3 | 5 |
| [2026-10-01-posting-tail-ab](../benches/results/2026-10-01-posting-tail-ab) | 1 | 20 | 47 | 20 | 41 |
| [2026-10-01-prepared-bulk-profile](../benches/results/2026-10-01-prepared-bulk-profile) | 1 | 0 | 1 | 0 | 0 |
| [2026-10-01-prepared-cells-ab](../benches/results/2026-10-01-prepared-cells-ab) | 1 | 6 | 14 | 6 | 6 |
| [2026-10-01-prepared-sorted-records](../benches/results/2026-10-01-prepared-sorted-records) | 1 | 12 | 26 | 12 | 16 |
| [2026-10-01-prepared-write-profile](../benches/results/2026-10-01-prepared-write-profile) | 1 | 0 | 1 | 0 | 0 |
| [2026-10-01-row-build-after](../benches/results/2026-10-01-row-build-after) | 1 | 1 | 2 | 1 | 0 |
| [2026-10-01-row-build-before](../benches/results/2026-10-01-row-build-before) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-segmented-embedding](../benches/results/2026-10-01-segmented-embedding) | 1 | 24 | 51 | 24 | 26 |
| [2026-10-01-serial-final-bulk](../benches/results/2026-10-01-serial-final-bulk) | 1 | 1 | 3 | 1 | 0 |
| [2026-10-01-shared-identity-ab](../benches/results/2026-10-01-shared-identity-ab) | 1 | 45 | 103 | 45 | 49 |
| [2026-10-01-shared-wal-vectors](../benches/results/2026-10-01-shared-wal-vectors) | 1 | 12 | 26 | 12 | 15 |
| [2026-10-01-sparse-single-pass](../benches/results/2026-10-01-sparse-single-pass) | 1 | 6 | 13 | 6 | 8 |
| [2026-10-01-sparse-vector-wal](../benches/results/2026-10-01-sparse-vector-wal) | 1 | 4 | 10 | 4 | 10 |
| [2026-10-01-stream-delete-ab](../benches/results/2026-10-01-stream-delete-ab) | 1 | 6 | 15 | 6 | 10 |
| [2026-10-01-substr-packed](../benches/results/2026-10-01-substr-packed) | 4 | 4 | 0 | 0 | 0 |
| [2026-10-01-substr-short-scan](../benches/results/2026-10-01-substr-short-scan) | 4 | 4 | 0 | 0 | 0 |
| [2026-10-01-wal-cell-refs](../benches/results/2026-10-01-wal-cell-refs) | 1 | 12 | 26 | 12 | 14 |
| [2026-10-01-wal-shared-text](../benches/results/2026-10-01-wal-shared-text) | 1 | 12 | 26 | 12 | 15 |
| [2026-10-01-wal-uniform-fields](../benches/results/2026-10-01-wal-uniform-fields) | 1 | 12 | 26 | 12 | 15 |
| [2026-10-01-wal-zstd](../benches/results/2026-10-01-wal-zstd) | 1 | 12 | 26 | 12 | 17 |
| [2026-10-01-write-after](../benches/results/2026-10-01-write-after) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-write-before-verified](../benches/results/2026-10-01-write-before-verified) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-write-borrowed-tokens](../benches/results/2026-10-01-write-borrowed-tokens) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-write-final](../benches/results/2026-10-01-write-final) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-01-write-owned-move](../benches/results/2026-10-01-write-owned-move) | 0 | 1 | 2 | 1 | 0 |
| [2026-10-02-merged-validation](../benches/results/2026-10-02-merged-validation) | 1 | 0 | 0 | 0 | 2 |
| [2026-10-02-mysql-owned-native](../benches/results/2026-10-02-mysql-owned-native) | 1 | 8 | 21 | 8 | 10 |
| [2026-10-03-embed-bigram-table](../benches/results/2026-10-03-embed-bigram-table) | 1 | 36 | 77 | 36 | 41 |
| [2026-10-03-embed-zero-marker](../benches/results/2026-10-03-embed-zero-marker) | 1 | 24 | 52 | 24 | 27 |
| [2026-10-03-fts-inline-postings](../benches/results/2026-10-03-fts-inline-postings) | 1 | 36 | 76 | 36 | 39 |
| [2026-10-03-fts-inline-tokens](../benches/results/2026-10-03-fts-inline-tokens) | 1 | 12 | 26 | 12 | 16 |
| [2026-10-03-fts-sorted-union](../benches/results/2026-10-03-fts-sorted-union) | 1 | 12 | 27 | 12 | 15 |
| [2026-10-03-insert-allocation-profile](../benches/results/2026-10-03-insert-allocation-profile) | 1 | 0 | 3 | 0 | 4 |
| [2026-10-03-insert-borrowed-text](../benches/results/2026-10-03-insert-borrowed-text) | 1 | 24 | 51 | 24 | 27 |
| [2026-10-03-insert-phase-profile](../benches/results/2026-10-03-insert-phase-profile) | 1 | 3 | 9 | 3 | 4 |
| [2026-10-03-lex-borrow-lower](../benches/results/2026-10-03-lex-borrow-lower) | 1 | 12 | 27 | 12 | 16 |
| [2026-10-03-mongo-native](../benches/results/2026-10-03-mongo-native) | 9 | 8 | 3 | 0 | 4 |
| [2026-10-03-parallel-row-build](../benches/results/2026-10-03-parallel-row-build) | 1 | 24 | 51 | 24 | 29 |
| [2026-10-03-peer-read-refresh](../benches/results/2026-10-03-peer-read-refresh) | 13 | 12 | 3 | 0 | 4 |
| [2026-10-03-shared-ast-strings](../benches/results/2026-10-03-shared-ast-strings) | 1 | 66 | 138 | 66 | 71 |
| [2026-10-03-wal-block-mask](../benches/results/2026-10-03-wal-block-mask) | 1 | 12 | 27 | 12 | 15 |
| [2026-10-03-wal-cached-counts](../benches/results/2026-10-03-wal-cached-counts) | 1 | 12 | 27 | 12 | 15 |
| [2026-10-03-wal-profile](../benches/results/2026-10-03-wal-profile) | 1 | 3 | 9 | 3 | 4 |
| [2026-10-03-wal-zero-blocks](../benches/results/2026-10-03-wal-zero-blocks) | 1 | 12 | 27 | 12 | 16 |
| [2026-10-04-batch-token-slots](../benches/results/2026-10-04-batch-token-slots) | 1 | 48 | 103 | 48 | 50 |
| [2026-10-04-current-insert-profile](../benches/results/2026-10-04-current-insert-profile) | 1 | 3 | 9 | 3 | 4 |
| [2026-10-04-current-peer-read-matrix](../benches/results/2026-10-04-current-peer-read-matrix) | 13 | 12 | 4 | 0 | 4 |
| [2026-10-04-durable-phase-refresh](../benches/results/2026-10-04-durable-phase-refresh) | 1 | 6 | 18 | 6 | 8 |
| [2026-10-04-embed-ascii-split](../benches/results/2026-10-04-embed-ascii-split) | 1 | 36 | 76 | 36 | 40 |
| [2026-10-04-embed-fixed-trigrams](../benches/results/2026-10-04-embed-fixed-trigrams) | 1 | 36 | 76 | 36 | 39 |
| [2026-10-04-embed-trigram-table](../benches/results/2026-10-04-embed-trigram-table) | 1 | 24 | 51 | 24 | 27 |
| [2026-10-04-four-way-sparse-wal](../benches/results/2026-10-04-four-way-sparse-wal) | 1 | 24 | 55 | 24 | 27 |
| [2026-10-04-fts-ascii-split](../benches/results/2026-10-04-fts-ascii-split) | 1 | 24 | 52 | 24 | 26 |
| [2026-10-04-mimalloc-prototype](../benches/results/2026-10-04-mimalloc-prototype) | 1 | 48 | 102 | 48 | 52 |
| [2026-10-04-normalization-half-density](../benches/results/2026-10-04-normalization-half-density) | 1 | 24 | 50 | 24 | 26 |
| [2026-10-04-parallel-sparse-classifier](../benches/results/2026-10-04-parallel-sparse-classifier) | 1 | 24 | 55 | 24 | 27 |
| [2026-10-04-parallel-sparse-wal](../benches/results/2026-10-04-parallel-sparse-wal) | 1 | 36 | 83 | 36 | 41 |
| [2026-10-04-row-major-wal-pack](../benches/results/2026-10-04-row-major-wal-pack) | 1 | 36 | 79 | 36 | 42 |
| [2026-10-04-row-worker-fallback](../benches/results/2026-10-04-row-worker-fallback) | 1 | 24 | 51 | 24 | 27 |
| [2026-10-04-row-worker-low-threshold](../benches/results/2026-10-04-row-worker-low-threshold) | 1 | 24 | 51 | 24 | 27 |
| [2026-10-04-scoped-batch-embed](../benches/results/2026-10-04-scoped-batch-embed) | 1 | 48 | 104 | 48 | 52 |
| [2026-10-04-small-sparse-backfill](../benches/results/2026-10-04-small-sparse-backfill) | 1 | 24 | 55 | 24 | 27 |
| [2026-10-04-vector-dictionary-audit](../benches/results/2026-10-04-vector-dictionary-audit) | 1 | 0 | 5 | 0 | 4 |
| [2026-10-04-vector-dictionary-buffered](../benches/results/2026-10-04-vector-dictionary-buffered) | 1 | 12 | 27 | 12 | 15 |
| [2026-10-04-vector-dictionary-codec](../benches/results/2026-10-04-vector-dictionary-codec) | 1 | 12 | 27 | 12 | 18 |
| [2026-10-04-wal-narrow-coordinates](../benches/results/2026-10-04-wal-narrow-coordinates) | 1 | 24 | 52 | 24 | 29 |
| [2026-10-04-wal-paired-write](../benches/results/2026-10-04-wal-paired-write) | 1 | 36 | 77 | 36 | 39 |
| [2026-10-04-wal-vectored-write](../benches/results/2026-10-04-wal-vectored-write) | 1 | 12 | 28 | 12 | 14 |
| [2026-10-05-durable-phase-refresh](../benches/results/2026-10-05-durable-phase-refresh) | 1 | 3 | 9 | 3 | 4 |
| [2026-10-05-sparse-size-reserve](../benches/results/2026-10-05-sparse-size-reserve) | 1 | 48 | 101 | 48 | 50 |

## Описание каждой серии

### 1. 0.4.0-core-phases

Артефакты: [0.4.0-core-phases](../benches/results/0.4.0-core-phases).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/cold_reopen_5k/lin`, `compare/hot_reopen_5k/lin`, `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/join_inner/lin_cursor`, `compare/join_phase/lin_cursor_open`, `compare/join_phase/lin_cursor_scan_project`, `compare/reopen_phase_5k/rebuild_fts`, `compare/reopen_phase_5k/rebuild_row_maps`, `compare/reopen_phase_5k/rebuild_scalar_indexes`.
- [progress.json](../benches/results/0.4.0-core-phases/progress.json)
- [run.json](../benches/results/0.4.0-core-phases/run.json)
- [status-final.json](../benches/results/0.4.0-core-phases/status-final.json)

### 2. 0.4.0-fts-phases

Артефакты: [0.4.0-fts-phases](../benches/results/0.4.0-fts-phases).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/fts_hybrid_common/lin`, `compare/fts_lex_common/lin`, `compare/fts_lex_miss/lin`, `compare/fts_lex_selective/lin`, `compare/reopen_phase_5k/rebuild_fts`.
- [progress.json](../benches/results/0.4.0-fts-phases/progress.json)
- [run.json](../benches/results/0.4.0-fts-phases/run.json)
- [status-final.json](../benches/results/0.4.0-fts-phases/status-final.json)

### 3. 0.4.0-full-70

Артефакты: [0.4.0-full-70](../benches/results/0.4.0-full-70).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/append_log_10k/duckdb`, `compare/append_log_10k/lin`, `compare/append_log_10k/mysql`, `compare/append_log_10k/postgres`, `compare/append_log_10k/sqlite`, `compare/append_log_1k/duckdb`, `compare/append_log_1k/lin`, `compare/append_log_1k/mysql`, `compare/append_log_1k/postgres`, `compare/append_log_1k/sqlite`, `compare/cold_reopen_5k/lin`, `compare/durable_append_10k/lin`, `compare/durable_append_10k/sqlite`, `compare/durable_append_1k/lin`, `compare/durable_append_1k/lin_normal_unsync`, `compare/durable_append_1k/sqlite`, `compare/durable_insert_10k/lin`, `compare/durable_insert_10k/sqlite`, `compare/durable_insert_1k/lin`, `compare/durable_insert_1k/sqlite`, `compare/filter_eq/duckdb`, `compare/filter_eq/lin`, `compare/filter_eq/mysql`, `compare/filter_eq/postgres`, `compare/filter_eq/sqlite`, `compare/filter_range/duckdb`, `compare/filter_range/lin`, `compare/filter_range/mysql`, `compare/filter_range/postgres`, `compare/filter_range/sqlite`, `compare/hot_reopen_5k/lin`, `compare/insert_bulk_10k/duckdb`, `compare/insert_bulk_10k/lin`, `compare/insert_bulk_10k/mysql`, `compare/insert_bulk_10k/postgres`, `compare/insert_bulk_10k/sqlite`, `compare/insert_bulk_1k/duckdb`, `compare/insert_bulk_1k/lin`, `compare/insert_bulk_1k/mysql`, `compare/insert_bulk_1k/postgres`, `compare/insert_bulk_1k/sqlite`, `compare/join_filter/duckdb`, `compare/join_filter/lin`, `compare/join_filter/lin_cursor`, `compare/join_filter/mysql`, `compare/join_filter/postgres`, `compare/join_filter/sqlite`, `compare/join_inner/duckdb`, `compare/join_inner/lin`, `compare/join_inner/lin_cursor`, `compare/join_inner/mysql`, `compare/join_inner/postgres`, `compare/join_inner/sqlite`, `compare/materialize/duckdb`, `compare/materialize/lin`, `compare/materialize/mysql`, `compare/materialize/postgres`, `compare/materialize/sqlite`, `compare/point_get/duckdb`, `compare/point_get/hashmap`, `compare/point_get/lin`, `compare/point_get/mysql`, `compare/point_get/postgres`, `compare/point_get/sqlite`, `compare/text_substr/duckdb`, `compare/text_substr/lin`, `compare/text_substr/mysql`, `compare/text_substr/postgres`, `compare/text_substr/sqlite`, `compare/wal_ship_1k/lin`.
- [progress.json](../benches/results/0.4.0-full-70/progress.json)
- [run.json](../benches/results/0.4.0-full-70/run.json)
- [status-final.json](../benches/results/0.4.0-full-70/status-final.json)

### 4. 0.4.0-insert-phases-repeat

Артефакты: [0.4.0-insert-phases-repeat](../benches/results/0.4.0-insert-phases-repeat).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/0.4.0-insert-phases-repeat/progress.json)
- [run.json](../benches/results/0.4.0-insert-phases-repeat/run.json)
- [status-final.json](../benches/results/0.4.0-insert-phases-repeat/status-final.json)

### 5. 0.4.0-join-phases-repeat

Артефакты: [0.4.0-join-phases-repeat](../benches/results/0.4.0-join-phases-repeat).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/join_inner/lin`, `compare/join_inner/lin_cursor`.
- [progress.json](../benches/results/0.4.0-join-phases-repeat/progress.json)
- [run.json](../benches/results/0.4.0-join-phases-repeat/run.json)
- [status-final.json](../benches/results/0.4.0-join-phases-repeat/status-final.json)

### 6. 2026-10-01-absolute-timestamps

Артефакты: [2026-10-01-absolute-timestamps](../benches/results/2026-10-01-absolute-timestamps).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-absolute-timestamps/report.md).

# Absolute timestamps and exact native-ingestion input parity

Added timestamp(signed_i64) syntax, representing Unix milliseconds with the existing time type and Cell::Time. Bare timestamp retains its former name parsing semantics. Type checking, canonical formatting, AST hashing, execution and scalar seek bounds handle the literal. Existing row/WAL/snapshot encodings are unchanged. README documents the syntax.

The benchmark source now uses Doc.ts directly for every Lin record. The warm fixture mutation/rebuild workaround was removed: assertions verify the values produced by actual inserts, including two [wing, ts] keys. Native Lin insertion validates exact timestamps rather than just checking Cell::Time. SQLite now has complete six-field readback validation like DuckDB Appender. Validation runs outside timing; no application result cache was introduced.

## Verification

The complete default workspace suite passed. New tests cover i64 boundaries, negative epochs, invalid float/duration/overflow forms, unchanged bare-name parsing, scan/index equality and ranges, prepared statements, bulk WAL replication, checkpoint and reopen. cargo check passed. Existing rollback, FTS, cold-store and large-insert count/WAL tests remain green.

The first broad benchmark selector accidentally included durable insertion and terminated at the existing 16MiB WAL-record ceiling. That incomplete run is excluded in excluded-run.json. The final precise selector contains only the six in-memory Lin/SQLite/DuckDB-Appender cases (selection.log); all completed with passing readback validators. This does not close the durable insertion requirement: its frame-size constraint still needs work.

## Exact-value native insertion snapshot

One process, 12 fresh fixtures per case, one operation each. Setup/input destruction and readback are outside timing; Appender flush is inside timing. Output types have trivial destructors. Shared host load is uncontrolled; this is not independent-process or statistical proof of a peer win. Earlier timing snapshots lacking exact timestamp checks are not equivalent exact-value evidence.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.028 | 0.820 | 1.263 |
| 10k | 10.990 | 10.488 | 10.443 |

No complete eight-peer victory is claimed. Native insertion/deletion optimization, the durable frame-size case, and live MSSQL/Kusto comparison remain outstanding. This change establishes exact data parity and a usable absolute-time input API; it is not a claimed performance speedup.

Raw observations, selection, build/test logs, excluded-run reason, before-source copies and final source hashes are retained.


</details>

### 7. 2026-10-01-ascii-lower-ab

Артефакты: [2026-10-01-ascii-lower-ab](../benches/results/2026-10-01-ascii-lower-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-ascii-lower-ab/report.md).

# Retained ASCII lowercase fast path

Lowercase ASCII borrows the source after byte classification; uppercase ASCII reuses scratch and make_ascii_lowercase. Unicode continues through the original per-character mapping. No content-hash algorithm or embedding formula changed.

Both binaries were rebuilt before measurement after a concurrent src/exec.rs change invalidated the first build pair. The matched current-build logs and current-common-source-sha256.json identify the actual compared state; initial build logs are historical and were not measured.

Three independent process pairs, reversing the second pair; 16 fresh fixtures per phase.

| Pair | Baseline full ms | ASCII full ms | Reduction | Baseline no embed/index/FTS ms | ASCII control ms |
|---|---:|---:|---:|---:|---:|
| 1 | 13.221 | 12.322 | 6.8% | 6.342 | 5.686 |
| 2 | 12.135 | 11.042 | 9.0% | 5.792 | 5.679 |
| 3 | 12.406 | 11.461 | 7.6% | 6.167 | 5.846 |

Median paired full-insertion reduction: 7.6%; all three pairs won. Controls also improved and background load is uncontrolled. The entire observed reduction cannot be confidently attributed to this patch, and no statistical significance or universal peer win is claimed.

## Fresh native peer validation

One process, 12 fresh fixtures, affected-count and row readback outside timing; Appender flush inside timing. Native work/schema differences are documented in the benchmark contract.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.072 | 0.785 | 1.199 |
| 10k | 11.666 | 10.856 | 10.409 |

Lin still loses both sizes to SQLite and 10k to Appender. A single-process 1k Appender win does not close the overall goal. Native delete and MSSQL/Kusto gaps remain.

Validation: complete default workspace tests passed after the external exec changes. The new test includes all 128 ASCII characters, controls, mixed case, Unicode expansions/Greek/Cyrillic, scratch reuse and borrowed pointer identity for unchanged ASCII. Existing independent embedding-bit/reference, FTS mutation/codec, rollback and durable-readback tests passed. Scoped rustfmt and git diff --check passed.


</details>

### 8. 2026-10-01-borrowed-delete-ab

Артефакты: [2026-10-01-borrowed-delete-ab](../benches/results/2026-10-01-borrowed-delete-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-borrowed-delete-ab/report.md).

# Borrowed moved-row deletion: retained

Deletion previously cloned the complete moved Row to edit its derived structures and then re-registered already transferred column values. The candidate reads the moved row in place while updating disjoint scalar/FTS fields, retains only shared identity keys for ID/URI/SPO maps, and uses the existing swap-pop of parallel columns. The identity maps keep remove/register semantics, including duplicate identities. Scalar insertion keeps the first-error boundary. Public Row and persistence formats remain unchanged.

The fast path checks all mirrored column lengths once before deletion. Nonparallel columns retain the old clone/register/rebuild path. No row is temporarily removed or replaced with an empty placeholder; this differs from the previous rejected mem::take moved-row experiment. Timing covers CAS, undo and full indexes/FTS as before; no feature was disabled.

Baseline and candidate were built from a frozen snapshot. Only src/store.rs differed, including tests. Default workspace tests passed for the initial candidate; final targeted tests passed after preserving the scalar first-error boundary. The integrated final workspace test result is in tests-live.log and validation.json.

The two new tests compare repeated multi-position deletions against rebuilt rows, maps, every mirrored column, scalar forward/reverse maps and exact FTS positions in docs, orders, users, facts and a custom collection. Duplicate/dead/out-of-range requested positions are included. A separate test exercises the nonparallel-column fallback.

## Native deletion

Three independent prebuilt process pairs with alternating order, 24 fresh samples per case, one timed operation each. Setup/destruction and existing affected/count/readback checks are outside timing. Our compilation/tests completed before benchmarking. Unrelated host activity remains uncontrolled; background process inventory is retained. In several cases SQLite controls improved too, so full timing reductions cannot be attributed solely to the code change; no statistical significance is claimed.

| Pair | Rows | Baseline Lin µs | Borrowed Lin µs | Reduction | Baseline SQLite µs | Candidate-process SQLite µs |
|---|---|---:|---:|---:|---:|---:|
| 1 | 1k | 8.979 | 5.042 | 43.9% | 5.792 | 2.812 |
| 1 | 10k | 13.854 | 12.209 | 11.9% | 8.208 | 7.521 |
| 1 | 100k | 18.625 | 19.812 | -6.4% | 16.583 | 16.729 |
| 2 | 1k | 12.209 | 5.667 | 53.6% | 7.438 | 3.792 |
| 2 | 10k | 15.416 | 13.688 | 11.2% | 13.896 | 11.250 |
| 2 | 100k | 22.145 | 21.312 | 3.8% | 16.167 | 18.771 |
| 3 | 1k | 12.563 | 7.979 | 36.5% | 4.021 | 3.333 |
| 3 | 10k | 16.396 | 13.959 | 14.9% | 12.541 | 10.688 |
| 3 | 100k | 23.459 | 22.146 | 5.6% | 15.521 | 18.500 |

## Decision

- 1k: 3/3 candidate wins, median paired time reduction 43.9%.
- 10k: 3/3 candidate wins, median paired time reduction 11.9%.
- 100k: 2/3 candidate wins, median paired time reduction 3.8%.

Retained as a reduction in row-cloning and redundant column registration with positive paired observations. The 100k result is mixed; no universal deletion acceleration is proved. Lin still loses SQLite in all nine candidate-process size comparisons, so deletion and the full eight-peer goal remain incomplete. No RSS/allocated-byte reduction was measured.

Source variants, common/binary hashes, six successful raw measured runs, build/test logs and a source archive are retained. Build-only Cargo invocations intentionally matched no cases and exited 1 after successful compilation. Live integration replaces only the deletion section and adds its tests, preserving other concurrent changes.


</details>

### 9. 2026-10-01-bulk-borrowed-tokens

Артефакты: [2026-10-01-bulk-borrowed-tokens](../benches/results/2026-10-01-bulk-borrowed-tokens).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_bulk_10k/duckdb`, `compare/insert_bulk_10k/lin`, `compare/insert_bulk_10k/sqlite`, `compare/insert_bulk_1k/duckdb`, `compare/insert_bulk_1k/lin`, `compare/insert_bulk_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-bulk-borrowed-tokens/progress.json)
- [run.json](../benches/results/2026-10-01-bulk-borrowed-tokens/run.json)
- [status-final.json](../benches/results/2026-10-01-bulk-borrowed-tokens/status-final.json)

### 10. 2026-10-01-bulk-exact-norm

Артефакты: [2026-10-01-bulk-exact-norm](../benches/results/2026-10-01-bulk-exact-norm).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_bulk_10k/duckdb`, `compare/insert_bulk_10k/lin`, `compare/insert_bulk_10k/sqlite`, `compare/insert_bulk_1k/duckdb`, `compare/insert_bulk_1k/lin`, `compare/insert_bulk_1k/sqlite`, `compare/insert_native_10k/duckdb_appender`, `compare/insert_native_1k/duckdb_appender`.
- [progress.json](../benches/results/2026-10-01-bulk-exact-norm/progress.json)
- [run.json](../benches/results/2026-10-01-bulk-exact-norm/run.json)
- [status-final.json](../benches/results/2026-10-01-bulk-exact-norm/status-final.json)

### 11. 2026-10-01-bulk-fts-append

Артефакты: [2026-10-01-bulk-fts-append](../benches/results/2026-10-01-bulk-fts-append).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-bulk-fts-append/report.md).

# Peer insertion after retained FTS append optimization

One process, 12 fresh fixtures per case, uncontrolled background load.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.096 ms | 0.781 ms | 1.232 ms |
| 10k | 14.636 ms | 10.621 ms | 10.480 ms |

Exact appender readback passed; setup/preparation outside timing, flush inside.
Lin includes default embedding and FTS; SQL engines have the plain fixture schema.
Lin still loses to both optimized SQL ingestion paths at 10k. The retained FTS
improvement is established against the original Lin in the separate alternating
pairs, not by comparing this run to older peer reports. MSSQL/Kusto unverified.


</details>

### 12. 2026-10-01-bulk-pack-tracking

Артефакты: [2026-10-01-bulk-pack-tracking](../benches/results/2026-10-01-bulk-pack-tracking).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-bulk-pack-tracking/report.md).

# Bulk insertion follow-up

One process, eight fresh fixtures per case; uncontrolled background load.

| 10k rows | Median |
|---|---:|
| Lin with default embedding and FTS | 12.536 ms |
| SQLite plain schema | 10.425 ms |
| DuckDB Appender plain schema | 10.438 ms |

Exact readback passed. Setup/preparation outside timing; appender flush inside.
Lin still loses at 10k. No universal peer win or causal allocation speedup is
claimed. This run precedes the subsequent FTS cancellation correctness fix.


</details>

### 13. 2026-10-01-bulk-tree-ab

Артефакты: [2026-10-01-bulk-tree-ab](../benches/results/2026-10-01-bulk-tree-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-bulk-tree-ab/report.md).

# Rejected sorted scalar-index slab construction

Three independent process pairs, alternating order, 12 fresh fixtures per case.

| Pair | Original full 10k | Sorted bulk build |
|---|---:|---:|
| 1 | 12.768 ms | 14.030 ms |
| 2 | 12.893 ms | 13.826 ms |
| 3 | 13.287 ms | 13.720 ms |

Process-median aggregate: 12.893 ms original vs 13.826 ms sorted.
The candidate lost all three full-insertion pairs and was removed. It passed
the workspace suite and direct parity/uniqueness tests. Sources, hashes and raw
runs are saved. No peer win was established. Background load is uncontrolled.


</details>

### 14. 2026-10-01-bulk-tree-before

Артефакты: [2026-10-01-bulk-tree-before](../benches/results/2026-10-01-bulk-tree-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-bulk-tree-before/progress.json)
- [run.json](../benches/results/2026-10-01-bulk-tree-before/run.json)
- [status-final.json](../benches/results/2026-10-01-bulk-tree-before/status-final.json)

### 15. 2026-10-01-bulk-wal-count-fixed

Артефакты: [2026-10-01-bulk-wal-count-fixed](../benches/results/2026-10-01-bulk-wal-count-fixed).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-bulk-wal-count-fixed/report.md).

# Bulk insertion after WAL and affected-count fix

One process, eight fresh fixtures per case. Lin checks affected count, total rows,
static fields, timestamp type and embedding presence outside timing. DuckDB
Appender exact readback also passed. Setup/preparation outside timing; flush inside.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.166 | 0.801 | 1.182 |
| 10k | 12.762 | 10.585 | 10.205 |

Lin performs default embedding and FTS; SQL peers have the plain fixture schema.
Lin still loses the 10k ingestion comparisons. This run establishes stronger
correctness checks, not a causal speedup. Background load is uncontrolled.

The separate WAL regression confirms all 129 inserted rows/fields/embeddings
reach a fresh replica even though Handle.rows is elided. The full peer goal
remains incomplete; MSSQL and Kusto lack configured dedicated endpoints.


</details>

### 16. 2026-10-01-bulk-with-appender

Артефакты: [2026-10-01-bulk-with-appender](../benches/results/2026-10-01-bulk-with-appender).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-bulk-with-appender/report.md).

# Bulk ingestion with native DuckDB Appender

Eight samples, one process; preparation outside timing.
DuckDB Appender is flushed inside the timer and all six columns are read back and compared outside it.
Lin full docs insertion includes embedding and FTS; SQL adapters do not.
SQL row-loop and appender are separate cases. Results vary between runs; no independent process confidence is claimed.

| Case | Median ms |
|---|---:|
| compare/insert_bulk_1k/lin | 1.727 |
| compare/insert_bulk_1k/sqlite | 1.006 |
| compare/insert_bulk_1k/duckdb | 176.460 |
| compare/insert_bulk_10k/lin | 18.755 |
| compare/insert_bulk_10k/sqlite | 14.432 |
| compare/insert_bulk_10k/duckdb | 2757.613 |
| compare/insert_native_1k/duckdb_appender | 1.617 |
| compare/insert_native_10k/duckdb_appender | 13.708 |


</details>

### 17. 2026-10-01-committed-local-read-audit

Артефакты: [2026-10-01-committed-local-read-audit](../benches/results/2026-10-01-committed-local-read-audit).

<details>
<summary>process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-committed-local-read-audit/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.243 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.23× | validated |
| point_get | duckdb | 35.972 | 148.33× | validated |
| point_get | pandas | 4.501 | 18.56× | validated |
| filter_eq | lin | 0.770 | 1.00× | validated |
| filter_eq | sqlite | 717.024 | 930.87× | validated |
| filter_eq | duckdb | 450.571 | 584.95× | validated |
| filter_eq | pandas | 2308.729 | 2997.30× | validated |
| text_substr | lin | 238.993 | 1.00× | validated |
| text_substr | sqlite | 3564.250 | 14.91× | validated |
| text_substr | duckdb | 477.338 | 2.00× | validated |
| text_substr | pandas | 4895.750 | 20.48× | validated |
| materialize | lin | 5468.542 | 1.00× | validated |
| materialize | sqlite | 22069.709 | 4.04× | validated |
| materialize | duckdb | 12186.458 | 2.23× | validated |
| materialize | pandas | 18140.125 | 3.32× | validated |
| join_inner | lin | 14411.625 | 1.00× | validated |
| join_inner | sqlite | 53254.458 | 3.70× | validated |
| join_inner | duckdb | 29549.834 | 2.05× | validated |
| join_inner | pandas | 36483.833 | 2.53× | validated |
| join_filter | lin | 7060.083 | 1.00× | validated |
| join_filter | sqlite | 26310.042 | 3.73× | validated |
| join_filter | duckdb | 14555.958 | 2.06× | validated |
| join_filter | pandas | 19290.750 | 2.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-committed-local-read-audit/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.036 | 4.18× | validated |
| point_get | duckdb | 35.663 | 144.07× | validated |
| point_get | pandas | 4.497 | 18.17× | validated |
| point_get | lin | 0.248 | 1.00× | validated |
| filter_eq | sqlite | 715.271 | 923.78× | validated |
| filter_eq | duckdb | 451.436 | 583.04× | validated |
| filter_eq | pandas | 2300.104 | 2970.62× | validated |
| filter_eq | lin | 0.774 | 1.00× | validated |
| text_substr | sqlite | 3524.042 | 14.40× | validated |
| text_substr | duckdb | 505.653 | 2.07× | validated |
| text_substr | pandas | 4958.875 | 20.26× | validated |
| text_substr | lin | 244.710 | 1.00× | validated |
| materialize | sqlite | 21051.000 | 3.63× | validated |
| materialize | duckdb | 12496.792 | 2.15× | validated |
| materialize | pandas | 17623.459 | 3.04× | validated |
| materialize | lin | 5802.625 | 1.00× | validated |
| join_inner | sqlite | 52053.125 | 3.64× | validated |
| join_inner | duckdb | 30452.916 | 2.13× | validated |
| join_inner | pandas | 36421.541 | 2.55× | validated |
| join_inner | lin | 14291.750 | 1.00× | validated |
| join_filter | sqlite | 25969.875 | 3.65× | validated |
| join_filter | duckdb | 14497.625 | 2.04× | validated |
| join_filter | pandas | 19789.375 | 2.78× | validated |
| join_filter | lin | 7117.292 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-committed-local-read-audit/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 34.964 | 142.20× | validated |
| point_get | pandas | 4.519 | 18.38× | validated |
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.17× | validated |
| filter_eq | duckdb | 440.462 | 565.71× | validated |
| filter_eq | pandas | 2278.146 | 2925.97× | validated |
| filter_eq | lin | 0.779 | 1.00× | validated |
| filter_eq | sqlite | 709.521 | 911.28× | validated |
| text_substr | duckdb | 476.967 | 2.05× | validated |
| text_substr | pandas | 5213.041 | 22.44× | validated |
| text_substr | lin | 232.300 | 1.00× | validated |
| text_substr | sqlite | 3483.167 | 14.99× | validated |
| materialize | duckdb | 12985.958 | 2.24× | validated |
| materialize | pandas | 18358.125 | 3.17× | validated |
| materialize | lin | 5787.125 | 1.00× | validated |
| materialize | sqlite | 21854.958 | 3.78× | validated |
| join_inner | duckdb | 29528.583 | 2.10× | validated |
| join_inner | pandas | 53917.833 | 3.83× | validated |
| join_inner | lin | 14059.625 | 1.00× | validated |
| join_inner | sqlite | 71023.250 | 5.05× | validated |
| join_filter | duckdb | 14924.625 | 2.00× | validated |
| join_filter | pandas | 19410.834 | 2.60× | validated |
| join_filter | lin | 7463.458 | 1.00× | validated |
| join_filter | sqlite | 26642.708 | 3.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-committed-local-read-audit/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.17× | validated |
| point_get | duckdb | 35.663 | 145.04× | validated |
| point_get | pandas | 4.501 | 18.30× | validated |
| filter_eq | lin | 0.774 | 1.00× | validated |
| filter_eq | sqlite | 715.271 | 923.78× | validated |
| filter_eq | duckdb | 450.571 | 581.92× | validated |
| filter_eq | pandas | 2300.104 | 2970.62× | validated |
| text_substr | lin | 238.993 | 1.00× | validated |
| text_substr | sqlite | 3524.042 | 14.75× | validated |
| text_substr | duckdb | 477.338 | 2.00× | validated |
| text_substr | pandas | 4958.875 | 20.75× | validated |
| materialize | lin | 5787.125 | 1.00× | validated |
| materialize | sqlite | 21854.958 | 3.78× | validated |
| materialize | duckdb | 12496.792 | 2.16× | validated |
| materialize | pandas | 18140.125 | 3.13× | validated |
| join_inner | lin | 14291.750 | 1.00× | validated |
| join_inner | sqlite | 53254.458 | 3.73× | validated |
| join_inner | duckdb | 29549.834 | 2.07× | validated |
| join_inner | pandas | 36483.833 | 2.55× | validated |
| join_filter | lin | 7117.292 | 1.00× | validated |
| join_filter | sqlite | 26310.042 | 3.70× | validated |
| join_filter | duckdb | 14555.958 | 2.05× | validated |
| join_filter | pandas | 19410.834 | 2.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

### 18. 2026-10-01-delete-owned-result-ab

Артефакты: [2026-10-01-delete-owned-result-ab](../benches/results/2026-10-01-delete-owned-result-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-delete-owned-result-ab/report.md).

# Rejected owned delete-result experiment

Three independent process pairs, alternating order, 40 fresh fixtures per case.
Each measured operation mutates a fresh input, so iteration count is capped at one.
Timings are short; scheduler/cache/background effects limit precision.

| Pair | Table rows | Original Lin µs | Owned Lin µs | Original SQLite µs | Owned-run SQLite µs |
|---|---:|---:|---:|---:|---:|
| 1 | 1k | 5.500 | 6.396 | 3.292 | 4.542 |
| 1 | 10k | 14.959 | 14.209 | 10.583 | 12.375 |
| 1 | 100k | 22.084 | 18.979 | 19.146 | 16.250 |
| 2 | 1k | 5.604 | 3.833 | 2.500 | 2.583 |
| 2 | 10k | 11.312 | 10.438 | 8.666 | 6.500 |
| 2 | 100k | 20.250 | 20.958 | 15.604 | 17.062 |
| 3 | 1k | 6.792 | 8.999 | 4.042 | 5.854 |
| 3 | 10k | 13.687 | 15.229 | 7.562 | 12.646 |
| 3 | 100k | 23.480 | 27.041 | 18.230 | 17.250 |

The candidate did not show consistent gains and regressed all table sizes in
pair 3. It was removed. It passed workspace tests; a new regression for removed
row order and rollback positions remains. No SQLite deletion win is claimed.
Sources, hashes and raw runs are retained. The full multi-engine goal is unmet.


</details>

### 19. 2026-10-01-delete-owned-result-after

Артефакты: [2026-10-01-delete-owned-result-after](../benches/results/2026-10-01-delete-owned-result-after).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-delete-owned-result-after/progress.json)
- [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json)
- [status-final.json](../benches/results/2026-10-01-delete-owned-result-after/status-final.json)

### 20. 2026-10-01-delete-owned-result-before

Артефакты: [2026-10-01-delete-owned-result-before](../benches/results/2026-10-01-delete-owned-result-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-delete-owned-result-before/progress.json)
- [run.json](../benches/results/2026-10-01-delete-owned-result-before/run.json)
- [status-final.json](../benches/results/2026-10-01-delete-owned-result-before/status-final.json)

### 21. 2026-10-01-dense-reverse-ab

Артефакты: [2026-10-01-dense-reverse-ab](../benches/results/2026-10-01-dense-reverse-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-dense-reverse-ab/report.md).

# Dense reverse scalar-index keys: rejected

An isolated candidate replaced the row-position FxHashMap<usize, IndexKey> with Vec<Option<IndexKey>>. Forward BTree postings, query bounds, numeric ordering, uniqueness checks, and remove/reinsert posting order were preserved. Holes supported updates, swap-removal and rollback. Production sources were not edited.

The complete default workspace suite passed. An additional 3,000-edit oracle covered insertion, replacement, deletion, sparse positions through 1024, clone independence and out-of-range removal. Pure dense insertion of extremely sparse arbitrary positions could allocate excessively, another issue that would need a bounded/sparse fallback before any production adoption.

Baseline and candidate were compiled from a frozen snapshot; only src/index.rs differed, including the extra test. Six independent prebuilt processes ran in three alternating pairs, with writes (12 fresh samples), insert phases (16 fresh samples), and warm scalar reads (24 calibrated samples). Mutation/insert operations were capped at one per fresh fixture. Existing count/readback validators ran outside timing. Builds and our tests ended before timing. Unrelated cargo/rustc processes were running on this shared host; the process inventory is retained. The timing variation does not establish statistical significance or an isolated causal percentage.

## Single-row writes

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.812 | 3.292 | 31.6% |
| 1 | update_1row_1k/sqlite | 5.125 | 3.354 | 34.5% |
| 1 | delete_1row_1k/lin | 10.959 | 8.709 | 20.5% |
| 1 | delete_1row_1k/sqlite | 8.791 | 4.542 | 48.3% |
| 1 | update_1row_10k/lin | 9.625 | 8.875 | 7.8% |
| 1 | update_1row_10k/sqlite | 14.563 | 9.875 | 32.2% |
| 1 | delete_1row_10k/lin | 16.709 | 13.062 | 21.8% |
| 1 | delete_1row_10k/sqlite | 12.125 | 11.562 | 4.6% |
| 1 | update_1row_100k/lin | 13.291 | 24.500 | -84.3% |
| 1 | update_1row_100k/sqlite | 23.499 | 18.250 | 22.3% |
| 1 | delete_1row_100k/lin | 28.041 | 33.167 | -18.3% |
| 1 | delete_1row_100k/sqlite | 19.062 | 18.375 | 3.6% |
| 2 | update_1row_1k/lin | 6.250 | 8.146 | -30.3% |
| 2 | update_1row_1k/sqlite | 5.188 | 7.458 | -43.8% |
| 2 | delete_1row_1k/lin | 7.938 | 13.438 | -69.3% |
| 2 | delete_1row_1k/sqlite | 5.854 | 7.167 | -22.4% |
| 2 | update_1row_10k/lin | 9.521 | 10.146 | -6.6% |
| 2 | update_1row_10k/sqlite | 13.354 | 11.313 | 15.3% |
| 2 | delete_1row_10k/lin | 18.646 | 16.291 | 12.6% |
| 2 | delete_1row_10k/sqlite | 12.188 | 16.959 | -39.1% |
| 2 | update_1row_100k/lin | 16.896 | 33.270 | -96.9% |
| 2 | update_1row_100k/sqlite | 18.084 | 23.646 | -30.8% |
| 2 | delete_1row_100k/lin | 23.834 | 41.874 | -75.7% |
| 2 | delete_1row_100k/sqlite | 18.354 | 22.584 | -23.0% |
| 3 | update_1row_1k/lin | 4.854 | 6.812 | -40.3% |
| 3 | update_1row_1k/sqlite | 4.312 | 3.958 | 8.2% |
| 3 | delete_1row_1k/lin | 8.812 | 5.688 | 35.5% |
| 3 | delete_1row_1k/sqlite | 4.667 | 4.229 | 9.4% |
| 3 | update_1row_10k/lin | 8.917 | 9.166 | -2.8% |
| 3 | update_1row_10k/sqlite | 10.667 | 11.271 | -5.7% |
| 3 | delete_1row_10k/lin | 14.312 | 14.437 | -0.9% |
| 3 | delete_1row_10k/sqlite | 13.021 | 9.500 | 27.0% |
| 3 | update_1row_100k/lin | 15.479 | 32.958 | -112.9% |
| 3 | update_1row_100k/sqlite | 19.396 | 18.333 | 5.5% |
| 3 | delete_1row_100k/lin | 23.980 | 36.416 | -51.9% |
| 3 | delete_1row_100k/sqlite | 19.104 | 16.729 | 12.4% |

## Insertion phases

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | insert_phase_10k/lin_full | 11777.208 | 15252.000 | -29.5% |
| 1 | insert_phase_10k/lin_no_embed | 8226.646 | 8850.833 | -7.6% |
| 1 | insert_phase_10k/lin_no_embed_no_scalar_index | 7093.395 | 8163.188 | -15.1% |
| 1 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 6147.374 | 6235.688 | -1.4% |
| 2 | insert_phase_10k/lin_full | 11338.729 | 18357.875 | -61.9% |
| 2 | insert_phase_10k/lin_no_embed | 7697.812 | 9989.833 | -29.8% |
| 2 | insert_phase_10k/lin_no_embed_no_scalar_index | 6773.938 | 8118.604 | -19.9% |
| 2 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5538.271 | 6148.000 | -11.0% |
| 3 | insert_phase_10k/lin_full | 12260.604 | 11849.146 | 3.4% |
| 3 | insert_phase_10k/lin_no_embed | 8133.730 | 7807.792 | 4.0% |
| 3 | insert_phase_10k/lin_no_embed_no_scalar_index | 7087.771 | 7130.999 | -0.6% |
| 3 | insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5625.958 | 6226.812 | -10.7% |

## Warm scalar equality read

| Pair | Case | Baseline µs | Dense reverse µs | Reduction |
|---|---|---:|---:|---:|
| 1 | filter_eq/lin | 1.039 | 1.150 | -10.7% |
| 2 | filter_eq/lin | 1.063 | 1.101 | -3.6% |
| 3 | filter_eq/lin | 1.063 | 1.137 | -6.9% |

## Decision

Rejected: no consistent improvement across required writes, insertion and reads, and no broad SQLite victory. The existing hash reverse map remains in the current workspace. Native write semantics, including undo/CAS/FTS, remain included; no feature was disabled to obtain timings. MSSQL/Kusto and the overall eight-peer goal remain unverified/incomplete. Build-only filtered Cargo calls exited 1 because they intentionally selected no cases after successful compilation; all 18 actual measurement runs completed with exit 0.


</details>

### 22. 2026-10-01-doc-slab-scan

Артефакты: [2026-10-01-doc-slab-scan](../benches/results/2026-10-01-doc-slab-scan).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-doc-slab-scan/report.md).

# One-pass document slab registration

The docs bulk registration path extracts id, uri, title, layer and wing with one row traversal instead of five BTreeMap lookups. Existing Arc text sharing, missing/nontext defaults, duplicate identity last-writer semantics and fallback behavior unchanged. Other collections unchanged.

Full offline workspace tests pass. Added 64-row reference test compares bulk registration to single-row registration with missing, null, integer, extra and repeated text values; checks both identity maps and all four mirrored document columns. Scoped formatting and diff checks pass.

Six alternating native process pairs and six durable pairs; 24 fresh fixtures per case, one operation. Exact field validation outside timer. Full default Lin behavior and durable fsync preserved. Pinned optimized binaries; tests/builds did not overlap timing. Host load uncontrolled, fixed case order within process. Milliseconds.

## Native inserts

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.025917 | 0.964875 | 11.011416 | 10.582125 |
| 2 | 1.025021 | 0.967270 | 11.331083 | 10.330229 |
| 3 | 1.013917 | 0.949104 | 11.048458 | 10.586354 |
| 4 | 1.009604 | 0.987708 | 11.343105 | 10.397042 |
| 5 | 1.038813 | 0.959041 | 11.410521 | 10.607541 |
| 6 | 1.064625 | 0.959729 | 11.102521 | 10.525583 |

Both sizes faster in 6/6 pairs; median paired decreases 6.17% (1k), 6.12% (10k). SQLite controls have median decreases -2.96% and +0.18%. Native 10k candidate beats SQLite in 4/6 same-process comparisons, DuckDB Appender only 1/6; not a stable peer victory.

| Rows | Candidate Lin median | SQLite same-process median | DuckDB Appender median |
|---|---:|---:|---:|
| 1k | 0.962302 | 0.828156 | 1.224448 |
| 10k | 10.553854 | 10.510906 | 10.361812 |

## Durable inserts

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.188480 | 2.394271 | 20.361000 | 19.851042 |
| 2 | 2.422937 | 2.431167 | 20.302188 | 19.153334 |
| 3 | 2.574625 | 3.878021 | 23.154896 | 25.872959 |
| 4 | 2.624521 | 2.366438 | 20.664667 | 19.468750 |
| 5 | 2.400354 | 2.067833 | 20.660792 | 19.820271 |
| 6 | 2.886917 | 3.317521 | 20.679042 | 19.774041 |

Durable 10k faster 5/6 pairs, median decrease 4.22%; durable 1k faster only 2/6, median decrease -4.87% (slower). Corresponding unchanged SQLite controls +2.11% and -5.47%; large outlier in pair 3. Do not attribute all changes to this code or claim causal significance. Retain the change for consistent native and large durable gains; small durable behavior remains unresolved. All-eight-peer objective remains unproven. Raw observations, baseline source and hashes retained.


</details>

### 23. 2026-10-01-duckdb-appender

Артефакты: [2026-10-01-duckdb-appender](../benches/results/2026-10-01-duckdb-appender).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_native_10k/duckdb_appender`, `compare/insert_native_1k/duckdb_appender`.
- [progress.json](../benches/results/2026-10-01-duckdb-appender/progress.json)
- [run.json](../benches/results/2026-10-01-duckdb-appender/run.json)
- [status-final.json](../benches/results/2026-10-01-duckdb-appender/status-final.json)

### 24. 2026-10-01-durable-profile

Артефакты: [2026-10-01-durable-profile](../benches/results/2026-10-01-durable-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-durable-profile/report.md).

# Durable insert profile and borrowed WAL field names

## Profile

Sampling example profile_durable ran 210 fresh 10k-document batches in 25.071 s. Timed insert calls totaled 6.823 s; setup 3.368 s. The remaining time includes validation, DB close/checkpoint, destruction and temporary-directory cleanup. This diagnostic is not a peer benchmark. sample.txt contains the full 15-second macOS stack capture.

The rows_to_insert_cols subtree contains BTreeSet<String> insertion, memcmp, allocation and free stacks. Existing code cloned every field name for every row, including duplicates. Changed the temporary union to BTreeSet<&str> and copy each unique field name only once into the owned pack. Field union, order, column inference, wire encoding and durability remain unchanged.

## Paired benchmark

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Exact row validation outside timer, Full fsync unchanged. Fixed per-process case order. Pinned optimized binaries; no build/test overlapped measurement. Host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 3.130791 | 2.570229 | 25.592875 | 24.452334 |
| 2 | 2.747416 | 2.607604 | 26.308813 | 23.823708 |
| 3 | 2.763417 | 2.545167 | 25.066063 | 24.835355 |
| 4 | 2.770459 | 2.576000 | 26.405729 | 23.757770 |
| 5 | 3.067541 | 3.591625 | 28.507458 | 60.798146 |
| 6 | 3.405958 | 3.321646 | 26.151521 | 31.751187 |

Observed Lin duration decreased in 5/6 pairs at 1k (median paired decrease 6.05%) and 4/6 at 10k (median paired decrease 2.69%). Pair 5 has a large 10k regression; pair 6 also regresses. Unchanged SQLite controls varied substantially and mostly slowed. These percentages are observations, not isolated causal attribution or significance claims. Retain the allocation-removing change provisionally; larger-workload consistency remains unresolved.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.591802 | 1.400771 |
| 10k | 24.643844 | 15.077678 |

Lin still loses these durable comparisons to SQLite. This does not prove superiority over the full eight-peer objective.

Full offline workspace tests pass. Scoped rustfmt and git diff checks pass. Raw run.json observations, benchmark logs, pre-change source, profile and binary/source hashes retained.


</details>

### 25. 2026-10-01-embed-alternating

Артефакты: [2026-10-01-embed-alternating](../benches/results/2026-10-01-embed-alternating).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-embed-alternating/report.md).

# Alternating embedding experiment

Three independent processes per variant; 12 fresh-input samples per case.
Variant order reversed in the second pair. All compilation finished before each measurement.
Substantial unrelated host CPU load remained; unchanged control cases drifted.
No reliable speedup was demonstrated, so the reciprocal experiment was removed.
No new peer win is claimed from this run. Exact variant sources and hashes are included.

| Process / variant | Full ms | No embedding ms | No embedding/index ms | No embedding/index/FTS ms |
|---|---:|---:|---:|---:|
| process-1-modulo | 18.353 | 12.583 | 10.996 | 9.448 |
| process-1-reciprocal | 34.464 | 12.307 | 10.808 | 7.853 |
| process-2-modulo | 18.986 | 12.072 | 11.031 | 7.601 |
| process-2-reciprocal | 22.874 | 20.174 | 11.143 | 7.900 |
| process-3-modulo | 18.713 | 12.726 | 10.548 | 7.450 |
| process-3-reciprocal | 17.541 | 11.946 | 10.874 | 7.700 |


</details>

### 26. 2026-10-01-embed-before

Артефакты: [2026-10-01-embed-before](../benches/results/2026-10-01-embed-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [measurement-limitations.json](../benches/results/2026-10-01-embed-before/measurement-limitations.json)
- [progress.json](../benches/results/2026-10-01-embed-before/progress.json)
- [run.json](../benches/results/2026-10-01-embed-before/run.json)
- [status-final.json](../benches/results/2026-10-01-embed-before/status-final.json)

### 27. 2026-10-01-embed-default-modulo

Артефакты: [2026-10-01-embed-default-modulo](../benches/results/2026-10-01-embed-default-modulo).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-embed-default-modulo/report.md).

# Default embedding dimension modulo specialization — rejected

Candidate explicitly selects constant modulo 768 for the default dimension, retaining dynamic modulo for other sizes. Slot semantics unchanged. Full workspace tests including bit-exact embedding references pass.

Six alternating process pairs; 24 fresh fixtures per case, one operation. Full native Lin, SQLite, DuckDB Appender paths with exact row checks outside timer. Schema/setup/drop excluded. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled; case order fixed. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.271396 | 1.118667 | 16.906645 | 11.590874 |
| 2 | 1.077666 | 1.049708 | 11.540417 | 11.624645 |
| 3 | 1.072583 | 1.074730 | 11.435104 | 11.554604 |
| 4 | 1.069605 | 1.082187 | 11.507021 | 11.785542 |
| 5 | 1.070771 | 1.065041 | 11.448688 | 11.520270 |
| 6 | 1.063167 | 1.075562 | 11.460646 | 11.574730 |

1k candidate wins 3/6 (median paired decrease 0.17%); 10k wins only 1/6 (median decrease -0.86%, slower). First pair shows large baseline variation shared with SQLite. No stable improvement established. Candidate rejected, original embedding source restored. All previously retained WAL changes remain. Native current-state comparison recorded separately in ../2026-10-01-native-after-wal/report.md. All-eight-peer goal remains unproven.


</details>

### 28. 2026-10-01-embed-pool-ab

Артефакты: [2026-10-01-embed-pool-ab](../benches/results/2026-10-01-embed-pool-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-embed-pool-ab/report.md).

# Reusable embedding pool: not retained

Three independent process pairs; 12 fresh-fixture samples per case. Times are medians in milliseconds.

| Pair | Serial full | Pool full | Serial no embedding | Pool no embedding |
|---|---:|---:|---:|---:|
| 1 | 13.744 | 13.812 | 8.954 | 10.000 |
| 2 | 17.809 | 29.664 | 79.140 | 13.140 |
| 3 | 51.878 | 19.100 | 22.987 | 15.975 |

The pool did not demonstrate a consistent win. Severe control variation prevents attributing the differences to the implementation. The candidate was reverted, including its direct Rayon dependency. The existing serial embedding optimizations and WAL/count correction remain.

Pool measurements follow benchmark validation/calibration and therefore measure a warm reusable pool. These results do not establish cold first-use latency, equal CPU cost, or wins against other engines. Source variants and raw observations remain in this directory.


</details>

### 29. 2026-10-01-embed-reciprocal

Артефакты: [2026-10-01-embed-reciprocal](../benches/results/2026-10-01-embed-reciprocal).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [measurement-limitations.json](../benches/results/2026-10-01-embed-reciprocal/measurement-limitations.json)
- [progress.json](../benches/results/2026-10-01-embed-reciprocal/progress.json)
- [run.json](../benches/results/2026-10-01-embed-reciprocal/run.json)
- [status-final.json](../benches/results/2026-10-01-embed-reciprocal/status-final.json)

### 30. 2026-10-01-embed-seen-after

Артефакты: [2026-10-01-embed-seen-after](../benches/results/2026-10-01-embed-seen-after).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-embed-seen-after/progress.json)
- [run.json](../benches/results/2026-10-01-embed-seen-after/run.json)
- [status-final.json](../benches/results/2026-10-01-embed-seen-after/status-final.json)

### 31. 2026-10-01-embed-seen-after-repeat

Артефакты: [2026-10-01-embed-seen-after-repeat](../benches/results/2026-10-01-embed-seen-after-repeat).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-embed-seen-after-repeat/report.md).

# Rejected accumulator marker experiment

Replacing the separate seen bitmap with a zero accumulator test preserved all
embedding unit/integration tests but did not establish a performance gain.

| Full 10k insertion | Before | Candidate |
|---|---:|---:|
| Initial pair | 18.323 ms | 12.815 ms |
| Repeat | 13.178 ms | 13.285 ms |

12 fresh fixtures per case in separate processes. Initial unchanged no-embedding
controls also sped up strongly (11.984 to 9.601 ms), invalidating a causal 30%
speedup claim. The repeat did not favor the candidate. The experiment was removed;
the retained source still uses the seen bitmap. No peer win was established.


</details>

### 32. 2026-10-01-embed-seen-before

Артефакты: [2026-10-01-embed-seen-before](../benches/results/2026-10-01-embed-seen-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-embed-seen-before/progress.json)
- [run.json](../benches/results/2026-10-01-embed-seen-before/run.json)
- [status-final.json](../benches/results/2026-10-01-embed-seen-before/status-final.json)

### 33. 2026-10-01-embed-seen-before-repeat

Артефакты: [2026-10-01-embed-seen-before-repeat](../benches/results/2026-10-01-embed-seen-before-repeat).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-embed-seen-before-repeat/progress.json)
- [run.json](../benches/results/2026-10-01-embed-seen-before-repeat/run.json)
- [status-final.json](../benches/results/2026-10-01-embed-seen-before-repeat/status-final.json)

### 34. 2026-10-01-embed-sparse-output

Артефакты: [2026-10-01-embed-sparse-output](../benches/results/2026-10-01-embed-sparse-output).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-embed-sparse-output/report.md).

# Sparse embedding output experiment — rejected

For sparse Scratch.finish outputs, initialize a new Arc slice with +0 and copy only touched coordinates; dense output path unchanged. Normalization and feature generation unchanged. Public Embedder API already returns Arc slices and was not changed.

Six alternating process pairs; 24 fresh fixtures per case, one operation. Exact rows checked outside timing, Full fsync unchanged. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.934292 | 2.889291 | 21.737958 | 22.491458 |
| 2 | 2.516230 | 2.768541 | 21.642459 | 21.981125 |
| 3 | 2.633979 | 2.657896 | 21.953771 | 21.404438 |
| 4 | 3.701145 | 2.583500 | 21.042792 | 21.325480 |
| 5 | 2.636938 | 2.645104 | 20.835938 | 21.737125 |
| 6 | 2.664521 | 2.729771 | 21.657979 | 21.790875 |

Candidate faster only 2/6 pairs at 1k, 1/6 at 10k. Median paired decreases -0.61% and -1.45% (slower). Unchanged SQLite controls varied; no isolated causal slowdown asserted. Candidate rejected, original embedding implementation restored. Previous WAL optimizations retained.

Full offline workspace tests pass, including bit-exact reference comparisons and normalization boundary cases. Formatting and diff checks pass. Raw observations, candidate and baseline source and binary hashes retained. All-eight-peer goal remains unproven.


</details>

### 35. 2026-10-01-empty-store-uniqueness

Артефакты: [2026-10-01-empty-store-uniqueness](../benches/results/2026-10-01-empty-store-uniqueness).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-empty-store-uniqueness/report.md).

# Empty-store uniqueness lookup bypass — rejected

Candidate skips existing-row ID/URI map lookups only when the target collection starts empty. Batch duplicate checks remain unconditional. Nonempty collections retain existing-row checks. No integrity or durability checks removed.

Full offline workspace tests pass. New test exercises duplicate ID and URI both within empty batches and against existing rows; checks unchanged rows/generation on failure and a successful retry. Test retained after reverting production candidate.

Six alternating process pairs, 24 fresh fixtures per case, one operation. Full native insert paths; exact field validation outside timer. Pinned optimized binaries; builds/tests did not overlap timings. Fixed case order, uncontrolled host load. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 0.925917 | 0.943958 | 9.825541 | 11.067854 |
| 2 | 0.945312 | 0.938146 | 9.737375 | 10.202000 |
| 3 | 0.920334 | 0.946167 | 10.115688 | 10.557375 |
| 4 | 0.989354 | 0.970459 | 10.962521 | 10.787104 |
| 5 | 0.976438 | 0.982563 | 10.108250 | 10.348437 |
| 6 | 0.974646 | 0.943937 | 10.284292 | 10.408563 |

1k faster 3/6, median paired decrease +0.07%; 10k faster only 1/6, median decrease -3.37% (slower). Unchanged SQLite/DuckDB controls also vary. No stable improvement; production candidate rejected and original exec source restored. Previously retained WAL and document registration improvements remain. All-eight-peer goal remains unproven.


</details>

### 36. 2026-10-01-fts-append-ab

Артефакты: [2026-10-01-fts-append-ab](../benches/results/2026-10-01-fts-append-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-append-ab/report.md).

# Retained FTS append fast path

Three independent process pairs, alternating order; 12 fresh fixtures per case.

| Pair | Original full 10k | FTS append | Improvement |
|---|---:|---:|---:|
| 1 | 14.224 ms | 13.764 ms | 3.2% |
| 2 | 13.828 ms | 12.654 ms | 8.5% |
| 3 | 12.887 ms | 12.305 ms | 4.5% |

Median paired improvement: 4.5%.
The candidate won all three full-insertion pairs and is retained. Background
load is uncontrolled; this is local evidence, not a universal peer win.
Fresh slabs without pending deltas append directly; recycled positions with
pending deltas use the general path. Tests cover Unicode, duplicate words,
tail recycling, fold parity and durable delete/checkpoint/reopen behavior.
Both source variants, hashes and raw runs are saved under this directory.


</details>

### 37. 2026-10-01-fts-append-before

Артефакты: [2026-10-01-fts-append-before](../benches/results/2026-10-01-fts-append-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-fts-append-before/progress.json)
- [run.json](../benches/results/2026-10-01-fts-append-before/run.json)
- [status-final.json](../benches/results/2026-10-01-fts-append-before/status-final.json)

### 38. 2026-10-01-fts-entry-ref

Артефакты: [2026-10-01-fts-entry-ref](../benches/results/2026-10-01-fts-entry-ref).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-entry-ref/report.md).

# Borrowed FTS hash entry experiment — rejected

Candidate replaces only the private postings map with hashbrown 0.17.1 and FxBuildHasher, and uses entry_ref to avoid a second hash-table lookup for newly seen tokens. Adds/dels remain the existing maps. Tokenization, sorted posting lists, query logic and durable format unchanged. Dependency and candidate source retained as artifacts, removed from production on rejection.

Full offline workspace tests pass, including FTS append/recycled-tail, pending-order, fold/codec and durable replay coverage. Formatting and diff checks pass. Pinned optimized binaries; builds/tests did not overlap benchmarks. Alternating process order, uncontrolled host load, fixed per-process case order.

## Native inserts

Six process pairs, 24 fresh fixtures per case, one operation each; full exact rows checked outside timer. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.029646 | 1.047667 | 10.969604 | 11.084376 |
| 2 | 1.011625 | 1.027583 | 10.634208 | 10.883000 |
| 3 | 1.003667 | 0.995541 | 10.770478 | 10.560645 |
| 4 | 1.004812 | 1.005271 | 10.355000 | 10.376397 |
| 5 | 1.060417 | 1.000959 | 10.388562 | 10.457438 |
| 6 | 1.027125 | 1.006917 | 10.735291 | 10.431625 |

1k faster 3/6, median paired decrease 0.38%; 10k faster 2/6, median decrease -0.43%. No stable insert gain.

## FTS reads and 10k single-row writes

Three process pairs, 16 samples each. Reads: batches capped at 64 iterations; writes: one operation/fresh fixture with readback checks. Microseconds per operation.

| Case | Baseline medians | Candidate medians |
|---|---|---|
| compare/fts_lex_selective/lin | 0.730, 0.725, 0.693 | 0.800, 0.755, 0.717 |
| compare/fts_lex_common/lin | 180.229, 176.042, 180.167 | 182.740, 177.724, 182.216 |
| compare/fts_lex_miss/lin | 0.276, 0.272, 0.271 | 0.284, 0.273, 0.272 |
| compare/update_1row_10k/lin | 8.562, 8.938, 8.459 | 8.979, 8.687, 9.250 |
| compare/update_1row_10k/sqlite | 9.854, 10.229, 8.709 | 10.771, 8.834, 11.625 |
| compare/delete_1row_10k/lin | 14.938, 14.438, 15.188 | 16.208, 15.229, 14.771 |
| compare/delete_1row_10k/sqlite | 10.625, 8.584, 10.021 | 8.812, 8.229, 10.542 |

Every measured FTS read case loses 3/3 pairs. Selective read median paired decrease -4.13% (slower). Update/delete each win only 1/3 pairs; controls also vary. Candidate rejected and original FTS map and Cargo files restored. Previous WAL optimizations retained. No all-eight-peer superiority established.


</details>

### 39. 2026-10-01-fts-fused-correct-ab

Артефакты: [2026-10-01-fts-fused-correct-ab](../benches/results/2026-10-01-fts-fused-correct-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-fused-correct-ab/report.md).

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


</details>

### 40. 2026-10-01-fts-inplace-fold-ab

Артефакты: [2026-10-01-fts-inplace-fold-ab](../benches/results/2026-10-01-fts-inplace-fold-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-inplace-fold-ab/report.md).

# In-place FTS fold: rejected

Baseline includes the sorted pending-edit correctness fix. Candidate reuses the term key, retains surviving positions in the base buffer, and avoids a second copy for an empty additions list.

Both release diagnostic binaries were built before measurement. Three independent process pairs, alternating the second pair, 12 seconds per process, 10000 resident documents. Each process validates row count, every ID/URI lookup, and exact lexical-search IDs/multiplicity after the measured loop. Other source hashes matched at candidate build.

| Pair | Baseline pairs/s | In-place pairs/s | Change |
|---|---:|---:|---:|
| 1 | 145342.4 | 139326.3 | -4.1% |
| 2 | 137655.2 | 136582.5 | -0.8% |
| 3 | 131639.1 | 151516.1 | 15.1% |

Not retained: two pairs lost and one won; background load is uncontrolled. No causal regression percentage or peer win is claimed. This diagnostic churn rate is not fresh-fixture single-delete latency and does not replace native peer benchmarks. Workspace tests passed on the candidate, including random pending edits, folding, codecs, rollback and durable readback.

The candidate was reverted to the correct sorted-pending baseline. The diagnostic lexical-search validation remains in examples/profile_writes.rs. Sources, hashes, build logs and all completed process logs are included.


</details>

### 41. 2026-10-01-fts-slab-scratch

Артефакты: [2026-10-01-fts-slab-scratch](../benches/results/2026-10-01-fts-slab-scratch).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-slab-scratch/report.md).

# Reuse FTS lowercase scratch across an appended slab

Retained: append_slab reuses one String for lowercase conversion across rows. Standalone append_row and pending-edit fallback keep previous semantics. No normalization, tokenization, posting or embedding semantics changed. Default lowercase fixture unchanged; opt-in LIN_BENCH_MIXED_CASE uses mixed-case titles for insertion-only comparisons and exact readback validation. Both pinned binaries include identical benchmark fixture changes.

Six alternating baseline/candidate process pairs per fixture, 24 fresh fixtures per case, one timed insert per fixture. Schema, preparation, validation and input destruction excluded; default embedding, FTS and scalar index enabled. No builds or tests overlapped timing. Fixed case order and uncontrolled host load; observations are not individual latency percentiles or evidence of universal peer victory. Values are median of process medians, milliseconds. Improvement is median paired relative decrease.

| Fixture | Rows | Baseline Lin | Candidate Lin | Paired decrease | Wins | SQLite baseline | SQLite candidate |
|---|---:|---:|---:|---:|---:|---:|---:|
| lower | 1k | 0.924958 | 0.931750 | -1.05% | 1/6 | 0.785771 | 0.800281 |
| lower | 10k | 9.942479 | 9.891365 | 1.07% | 4/6 | 10.276281 | 10.260479 |
| mixed | 1k | 0.962843 | 0.930052 | 2.96% | 6/6 | 0.790021 | 0.791990 |
| mixed | 10k | 10.229552 | 9.826375 | 3.90% | 6/6 | 10.346823 | 10.295250 |

Mixed-case Lin improves 6/6 pairs at both sizes; unchanged SQLite controls have paired median decreases -0.22% and +0.34%. Lowercase 1k Lin slower in 5/6 pairs (-1.05% median), but control SQLite also slower in 5/6 (-1.89%); lowercase 10k inconclusive (+1.07%, 4/6). Retain for repeatable mixed-case benefit without claiming lowercase improvement. SQLite still wins 1k in both fixtures. Durable writes and all eight peers were not measured in this experiment.

Validation: exact six-field insert readback outside timing in all 24 complete processes. Unit reference checks slab versus individual registration with changing buffer lengths, ASCII uppercase/lowercase, empty fields, Unicode expansion and snippet. Full offline workspace tests pass (tests-workspace.log); scoped rustfmt and git diff --check pass.


</details>

### 42. 2026-10-01-fts-sorted-pending

Артефакты: [2026-10-01-fts-sorted-pending](../benches/results/2026-10-01-fts-sorted-pending).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-fts-sorted-pending/report.md).

# Sorted FTS pending edits: correctness fix

The pre-existing merge helpers require sorted, unique inputs. Pending additions/removals were appended without sorting; cancellation used swap_remove, which also broke order. Deleting positions 8 then 2 from a 0..9 posting list left position 2 in matches. The new regression failed on the original two-pass implementation as well as the proposed fused implementation.

Pending insertion now uses binary_search plus ordered insert, deduplicating repeated edits. Cancellation uses binary_search and ordered remove. The fused-merge experiment was removed; performance comparisons must use a correct baseline.

Validation: workspace tests passed after the production fix. Two targeted regression tests passed on the final source: out-of-order edits with cancellations; descending removals past the fold threshold and 3000 deterministic random edits checked against a BTreeSet oracle at each step, plus fold/codec roundtrip.

## Current native deletion

One process, 12 fresh-fixture observations per case. Readback and affected counts are checked outside timing. This is verification, not a paired speedup claim.

| Rows | Lin µs | SQLite µs |
|---|---:|---:|
| 1k | 9.104 | 4.729 |
| 10k | 13.166 | 10.396 |
| 100k | 21.395 | 16.521 |

No universal native-delete win is established. Existing historical FTS mutation profiles/timings did not prove arbitrary-order pending correctness. The fix preserves the base-only persisted format; it does not repair already-corrupted derived posting blobs automatically.

Scoped rustfmt and git diff --check passed. Concurrent workspace work added an optional GPU module; a transient cargo fmt --all failure reported its then-missing file. Those changes were preserved; the checks here use the default feature set and do not establish GPU validation.


</details>

### 43. 2026-10-01-index-move-ab

Артефакты: [2026-10-01-index-move-ab](../benches/results/2026-10-01-index-move-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-index-move-ab/report.md).

# Scalar-index position move: rejected

Three alternating independent prebuilt process pairs, 16 fresh fixtures per case. Setup/destruction and readback validators are outside timing. The candidate moves reverse/forward positions without recreating the key; it preserves remove/reinsert posting order.

| Pair | Rows | Baseline Lin µs | Candidate Lin µs | Baseline SQLite µs | Candidate SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | 1k | 10.146 | 10.438 | 4.229 | 7.708 |
| 1 | 10k | 17.230 | 16.709 | 11.375 | 13.146 |
| 1 | 100k | 28.250 | 30.229 | 21.084 | 19.166 |
| 2 | 1k | 8.375 | 9.500 | 5.417 | 5.750 |
| 2 | 10k | 14.396 | 13.041 | 13.479 | 12.750 |
| 2 | 100k | 22.604 | 27.188 | 18.666 | 18.521 |
| 3 | 1k | 8.438 | 8.896 | 3.896 | 4.500 |
| 3 | 10k | 14.604 | 13.229 | 10.250 | 10.104 |
| 3 | 100k | 24.104 | 19.270 | 16.875 | 18.895 |

The 10k case won all three pairs, but 1k lost all three and 100k lost two of three. Overall native-delete wins were not demonstrated. The candidate was reverted. Background load is uncontrolled, so no causal regression percentage is claimed. The workspace suite and all sampled readback checks passed. Exact sources/hashes and raw runs remain. The prior inline-key/search-bound optimization remains retained.


</details>

### 44. 2026-10-01-index-reserve-ab

Артефакты: [2026-10-01-index-reserve-ab](../benches/results/2026-10-01-index-reserve-ab).

<details>
<summary>initial-report.md</summary>

Источник: [initial-report.md](../benches/results/2026-10-01-index-reserve-ab/initial-report.md).

# Reverse-index reservation experiment

Six independent prebuilt process pairs, alternating order; 12 fresh fixtures per phase.

| Pair | Baseline full ms | Reserve full ms | Improvement |
|---|---:|---:|---:|
| 1 | 18.019 | 38.194 | -112.0% |
| 2 | 21.584 | 18.726 | 13.2% |
| 3 | 38.874 | 48.327 | -24.3% |
| 4 | 24.158 | 24.104 | 0.2% |
| 5 | 22.378 | 32.946 | -47.2% |
| 6 | 26.761 | 29.453 | -10.1% |


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-index-reserve-ab/report.md).

# Reverse-index reservation experiment

Six independent prebuilt process pairs, alternating order; 12 fresh fixtures per phase.

| Pair | Baseline full ms | Reserve full ms | Improvement |
|---|---:|---:|---:|
| 1 | 18.019 | 38.194 | -112.0% |
| 2 | 21.584 | 18.726 | 13.2% |
| 3 | 38.874 | 48.327 | -24.3% |
| 4 | 24.158 | 24.104 | 0.2% |
| 5 | 22.378 | 32.946 | -47.2% |
| 6 | 26.761 | 29.453 | -10.1% |

Not retained: only two of six full-insert pairs won, with strongly unstable timings. Reservation was reverted. No causal regression percentage or peer win is claimed. Focused index tests passed; the source variants and raw observations remain.


</details>

### 45. 2026-10-01-index-same-key

Артефакты: [2026-10-01-index-same-key](../benches/results/2026-10-01-index-same-key).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-index-same-key/report.md).

# Posting-offset reverse index experiment — rejected

Three alternating process pairs, 12 fresh fixtures per case, one operation per fixture. Exact absolute timestamp fixtures and readback checks unchanged. Pinned optimized binaries; tests/builds did not overlap measurement. Host load uncontrolled. Numbers in microseconds.

Candidate stores (key, posting offset) in the reverse hash map and updates the swapped tail offset on removal. This removes linear posting search but enlarges reverse-map entries.

| Case | Baseline process medians | Candidate process medians |
|---|---|---|
| compare/update_1row_1k/lin | 4.583, 12.417, 10.062 | 4.708, 10.396, 8.791 |
| compare/delete_1row_1k/lin | 9.979, 10.271, 14.916 | 10.792, 16.104, 14.584 |
| compare/update_1row_10k/lin | 8.416, 13.188, 12.000 | 9.396, 13.708, 14.854 |
| compare/delete_1row_10k/lin | 16.541, 27.917, 20.375 | 14.334, 24.749, 21.562 |
| compare/update_1row_100k/lin | 13.416, 20.625, 20.729 | 23.375, 22.938, 19.584 |
| compare/delete_1row_100k/lin | 35.396, 53.771, 46.541 | 35.604, 33.646, 26.709 |

100k delete improved 2/3 pairs, but 10k update regressed 3/3 and 100k update regressed 2/3. Results do not establish an overall write improvement. Candidate rejected and original production representation restored. Unchanged SQLite controls and raw observations are retained. No all-peer superiority established.

Validation adds a 3000-operation model for reverse offsets and checks stored positions against forward postings; rebuilding may produce different posting order. Candidate source retained as candidate-index.rs.


</details>

### 46. 2026-10-01-index-visit-ab

Артефакты: [2026-10-01-index-visit-ab](../benches/results/2026-10-01-index-visit-ab).

<details>
<summary>peer-environment.md</summary>

Источник: [peer-environment.md](../benches/results/2026-10-01-index-visit-ab/peer-environment.md).

# Remaining peer environment requirements

Current local host reports arm64. Benchmark connection variable presence is recorded as booleans only in peer-env-presence.json; no secrets were captured. These are environment observations, not a claim that no other credentials exist anywhere.

Microsoft SQL Server Linux containers support Intel/AMD x86-64 Linux hosts, while Rosetta/QEMU translation is not supported or tested: https://learn.microsoft.com/en-us/sql/linux/quickstart-install-connect-docker?view=sql-server-ver17

The Kusto emulator requires SSE4.2/AVX2 and does not support ARM: https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install

The Kusto emulator has a different performance profile from Azure Data Explorer: https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview

Therefore local emulator timings on this host cannot close the live MSSQL/Kusto comparison requirement. A suitable native test environment and dedicated benchmark database connection are still required. The existing peer harness accepts these configurations. This gap does not block optimization against available peers and does not close or narrow the eight-peer objective.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-index-visit-ab/report.md).

# Direct scalar-index iteration: retained

Store::index_insert_row now iterates BTreeMap values_mut directly instead of allocating a Vec<String>, cloning matching index labels, and looking up each label again. Filtering, label order, first-error handling and unique-index semantics remain unchanged. Public row and persistence formats are unchanged. This is a removal of temporary allocations, not a measured byte/RSS reduction.

Baseline and candidate came from one frozen snapshot, only src/store.rs differing. The complete default workspace tests passed on that snapshot, including uniqueness, rollback, delete/readback, cold rows, checkpoint and WAL. Production integration is a narrow method replacement preserving other concurrent work; final live test status is recorded separately in tests-live.log and validation.json.

Six alternating independent process pairs, 16 fresh fixtures per case and one timed operation each. Setup/drop and existing affected-row/readback validation are outside timing. Our builds and tests ended before timing. Unrelated host jobs remain uncontrolled and their inventory is retained. SQLite controls changed substantially in several pairs, so these measurements do not isolate a causal percentage or establish statistical significance.

## Paired outcomes

| Case | Candidate wins / 6 | Median paired time reduction |
|---|---:|---:|
| update_1row_1k/lin | 4 | 15.3% |
| delete_1row_1k/lin | 3 | 2.3% |
| update_1row_10k/lin | 4 | 3.1% |
| delete_1row_10k/lin | 3 | 1.2% |
| update_1row_100k/lin | 5 | 2.3% |
| delete_1row_100k/lin | 3 | 1.9% |

100k updates won 5/6 baseline comparisons; the reduction is modest (2.3% median paired). At 1k and 10k the update candidate won 4/6. Deletion won only 3/6 at every size, so a repeatable delete gain is not proved. This change is retained for simpler allocation-free index dispatch and the positive update observations, with no claim of stable deletion acceleration.

## All measured medians

| Pair | Case | Baseline µs | Direct iteration µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.667 | 4.041 | 13.4% |
| 1 | update_1row_1k/sqlite | 3.792 | 4.000 | -5.5% |
| 1 | delete_1row_1k/lin | 7.500 | 6.833 | 8.9% |
| 1 | delete_1row_1k/sqlite | 5.292 | 3.167 | 40.2% |
| 1 | update_1row_10k/lin | 9.250 | 8.583 | 7.2% |
| 1 | update_1row_10k/sqlite | 12.021 | 11.771 | 2.1% |
| 1 | delete_1row_10k/lin | 14.500 | 15.062 | -3.9% |
| 1 | delete_1row_10k/sqlite | 11.500 | 11.688 | -1.6% |
| 1 | update_1row_100k/lin | 14.521 | 12.666 | 12.8% |
| 1 | update_1row_100k/sqlite | 16.645 | 17.084 | -2.6% |
| 1 | delete_1row_100k/lin | 20.750 | 27.291 | -31.5% |
| 1 | delete_1row_100k/sqlite | 16.770 | 19.041 | -13.5% |
| 2 | update_1row_1k/lin | 6.167 | 4.354 | 29.4% |
| 2 | update_1row_1k/sqlite | 5.667 | 3.979 | 29.8% |
| 2 | delete_1row_1k/lin | 5.812 | 8.291 | -42.6% |
| 2 | delete_1row_1k/sqlite | 4.938 | 3.146 | 36.3% |
| 2 | update_1row_10k/lin | 8.479 | 8.291 | 2.2% |
| 2 | update_1row_10k/sqlite | 10.146 | 11.188 | -10.3% |
| 2 | delete_1row_10k/lin | 14.604 | 13.979 | 4.3% |
| 2 | delete_1row_10k/sqlite | 10.125 | 10.646 | -5.1% |
| 2 | update_1row_100k/lin | 13.396 | 11.479 | 14.3% |
| 2 | update_1row_100k/sqlite | 16.667 | 17.520 | -5.1% |
| 2 | delete_1row_100k/lin | 29.562 | 23.396 | 20.9% |
| 2 | delete_1row_100k/sqlite | 17.104 | 17.562 | -2.7% |
| 3 | update_1row_1k/lin | 5.729 | 5.958 | -4.0% |
| 3 | update_1row_1k/sqlite | 3.875 | 2.562 | 33.9% |
| 3 | delete_1row_1k/lin | 6.583 | 8.312 | -26.3% |
| 3 | delete_1row_1k/sqlite | 5.021 | 2.792 | 44.4% |
| 3 | update_1row_10k/lin | 8.396 | 9.271 | -10.4% |
| 3 | update_1row_10k/sqlite | 11.041 | 11.000 | 0.4% |
| 3 | delete_1row_10k/lin | 13.875 | 14.125 | -1.8% |
| 3 | delete_1row_10k/sqlite | 8.625 | 11.125 | -29.0% |
| 3 | update_1row_100k/lin | 13.459 | 12.958 | 3.7% |
| 3 | update_1row_100k/sqlite | 15.209 | 17.750 | -16.7% |
| 3 | delete_1row_100k/lin | 20.438 | 24.230 | -18.6% |
| 3 | delete_1row_100k/sqlite | 17.250 | 17.959 | -4.1% |
| 4 | update_1row_1k/lin | 4.625 | 3.833 | 17.1% |
| 4 | update_1row_1k/sqlite | 4.083 | 3.625 | 11.2% |
| 4 | delete_1row_1k/lin | 7.458 | 6.729 | 9.8% |
| 4 | delete_1row_1k/sqlite | 3.896 | 3.042 | 21.9% |
| 4 | update_1row_10k/lin | 7.792 | 7.896 | -1.3% |
| 4 | update_1row_10k/sqlite | 9.729 | 7.542 | 22.5% |
| 4 | delete_1row_10k/lin | 14.771 | 14.021 | 5.1% |
| 4 | delete_1row_10k/sqlite | 10.625 | 7.167 | 32.6% |
| 4 | update_1row_100k/lin | 13.041 | 14.584 | -11.8% |
| 4 | update_1row_100k/sqlite | 21.979 | 22.312 | -1.5% |
| 4 | delete_1row_100k/lin | 28.562 | 27.125 | 5.0% |
| 4 | delete_1row_100k/sqlite | 19.250 | 18.708 | 2.8% |
| 5 | update_1row_1k/lin | 5.833 | 3.458 | 40.7% |
| 5 | update_1row_1k/sqlite | 5.104 | 2.917 | 42.9% |
| 5 | delete_1row_1k/lin | 7.646 | 7.979 | -4.4% |
| 5 | delete_1row_1k/sqlite | 5.938 | 3.563 | 40.0% |
| 5 | update_1row_10k/lin | 8.875 | 7.854 | 11.5% |
| 5 | update_1row_10k/sqlite | 10.334 | 9.292 | 10.1% |
| 5 | delete_1row_10k/lin | 14.959 | 13.250 | 11.4% |
| 5 | delete_1row_10k/sqlite | 9.500 | 11.666 | -22.8% |
| 5 | update_1row_100k/lin | 13.979 | 13.979 | 0.0% |
| 5 | update_1row_100k/sqlite | 16.645 | 17.479 | -5.0% |
| 5 | delete_1row_100k/lin | 22.959 | 19.563 | 14.8% |
| 5 | delete_1row_100k/sqlite | 19.041 | 18.709 | 1.7% |
| 6 | update_1row_1k/lin | 4.167 | 6.021 | -44.5% |
| 6 | update_1row_1k/sqlite | 6.042 | 4.271 | 29.3% |
| 6 | delete_1row_1k/lin | 12.896 | 10.396 | 19.4% |
| 6 | delete_1row_1k/sqlite | 3.521 | 4.146 | -17.8% |
| 6 | update_1row_10k/lin | 8.729 | 8.375 | 4.1% |
| 6 | update_1row_10k/sqlite | 12.500 | 10.479 | 16.2% |
| 6 | delete_1row_10k/lin | 14.458 | 15.604 | -7.9% |
| 6 | delete_1row_10k/sqlite | 12.771 | 13.500 | -5.7% |
| 6 | update_1row_100k/lin | 16.000 | 15.854 | 0.9% |
| 6 | update_1row_100k/sqlite | 17.375 | 22.291 | -28.3% |
| 6 | delete_1row_100k/lin | 22.062 | 22.354 | -1.3% |
| 6 | delete_1row_100k/sqlite | 16.792 | 17.083 | -1.7% |

## Peer status

Candidate-process Lin versus SQLite wins out of six: {'compare/update_1row_1k/lin': 0, 'compare/delete_1row_1k/lin': 0, 'compare/update_1row_10k/lin': 5, 'compare/delete_1row_10k/lin': 0, 'compare/update_1row_100k/lin': 6, 'compare/delete_1row_100k/lin': 0}. Native deletion still loses SQLite; an all-case victory remains unproved. The broader DuckDB/PostgreSQL/MySQL/MSSQL/Mongo/Kusto/pandas objective remains active and incomplete. See peer-environment.md for verified remaining MSSQL/Kusto platform/configuration gaps.

Build-only Cargo calls intentionally selected no cases and returned exit 1 after successful compilation. All 12 measured processes completed successfully. Compared source variants, common/binary hashes, build/test logs and raw run.json files are preserved.


</details>

### 47. 2026-10-01-inline-index-ab

Артефакты: [2026-10-01-inline-index-ab](../benches/results/2026-10-01-inline-index-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-inline-index-ab/report.md).

# Retained short index keys and search bounds

Index keys and search-bound temporaries use SmallVec with two inline parts. Larger composite keys spill to a heap buffer. The private key type preserves lexicographic ordering and numeric representation.

## Insertion key storage

Three initial process pairs won full 10k insertion by 6.5%, 11.1%, 6.7%. Builds between these runs are a possible disturbance. A second experiment built both binaries before measurements, then alternated order across six independent process pairs (12 fresh fixtures per case).

| Pair | Vec full, ms | Inline full, ms | Improvement |
|---|---:|---:|---:|
| 1 | 17.097 | 14.913 | 12.8% |
| 2 | 16.391 | 15.676 | 4.4% |
| 3 | 14.879 | 15.961 | -7.3% |
| 4 | 14.870 | 14.115 | 5.1% |
| 5 | 13.783 | 14.498 | -5.2% |
| 6 | 15.764 | 15.393 | 2.4% |

Median paired full-insert improvement: 3.4%; four of six pairs won. No-embedding improvement: 3.7%; unchanged no-scalar-index control: -3.1% (slower). Background load is uncontrolled; this is local evidence, not a universal causal percentage.

## Final search bounds

The final source also keeps equality prefixes and lower/upper search bounds inline. Both final and original Vec binaries were built before running six alternating independent process pairs, with 24 calibrated samples per case.

| Case | Vec process-median aggregate | Final process-median aggregate | Median paired improvement | Pairs won |
|---|---:|---:|---:|---:|
| filter_eq | 1.318 µs | 1.165 µs | 10.6% | 6/6 |
| filter_range | 2.244 µs | 2.147 µs | 4.3% | 5/6 |

## Final native ingestion

One independent process, 12 fresh-fixture samples per case. Affected count and row readback validators passed outside timing. DuckDB Appender includes flush inside timing.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.543 ms | 1.132 ms | 1.522 ms |
| 10k | 19.440 ms | 16.477 ms | 13.048 ms |

Lin still loses native ingestion comparisons. Native schema work differs: Lin computes embeddings and maintains FTS/indexes as documented in the contract. No claim of beating all peers is made. MSSQL/Kusto live comparisons remain missing.

## Validation

`cargo test --workspace --offline` passed on the final source. The new unit test checks short/spilled key ordering against Vec, unique conflicts, reverse-key replacement and deletion for arities 1, 2, 3, 5. Existing index, mutation, rollback, numeric and persistence tests passed. `cargo fmt --all -- --check` and `git diff --check` passed. Sources, hashes, raw runs, build logs and prebuilt replay script are included. The build-only filter deliberately selects no cases and can exit nonzero; build success is checked by the executable emitted by Cargo.


</details>

### 48. 2026-10-01-insert-borrowed-text

Артефакты: [2026-10-01-insert-borrowed-text](../benches/results/2026-10-01-insert-borrowed-text).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-insert-borrowed-text/report.md).

# Borrowed embedding text follow-up

One process, eight observations per case; background load uncontrolled.
Full 10k insertion: 18.447 ms; no embedding: 11.526 ms; no embedding or
scalar index: 9.894 ms; neither those nor FTS: 7.985 ms.
This diagnostic run has no paired baseline and proves no speedup or peer win.
The change removes text allocation for a single effective embedding field;
multi-field concatenation preserves the original separators.


</details>

### 49. 2026-10-01-insert-phases

Артефакты: [2026-10-01-insert-phases](../benches/results/2026-10-01-insert-phases).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-insert-phases/progress.json)
- [run.json](../benches/results/2026-10-01-insert-phases/run.json)
- [status-final.json](../benches/results/2026-10-01-insert-phases/status-final.json)

### 50. 2026-10-01-insert-profile

Артефакты: [2026-10-01-insert-profile](../benches/results/2026-10-01-insert-profile).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`.
- [progress.json](../benches/results/2026-10-01-insert-profile/progress.json)
- [run.json](../benches/results/2026-10-01-insert-profile/run.json)
- [status-final.json](../benches/results/2026-10-01-insert-profile/status-final.json)

### 51. 2026-10-01-insert-templates-ab

Артефакты: [2026-10-01-insert-templates-ab](../benches/results/2026-10-01-insert-templates-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-insert-templates-ab/report.md).

# Rejected prepared insertion templates

Three independent process pairs, alternating variant order; 12 fresh fixtures
per case. Both sources and SHA-256 hashes are saved under variants/.

| Pair | Original full 10k | Templates full 10k |
|---|---:|---:|
| 1 | 12.996 ms | 12.765 ms |
| 2 | 13.459 ms | 13.371 ms |
| 3 | 13.320 ms | 13.802 ms |

Process-median aggregates: original 13.320 ms, templates 13.371 ms.
The candidate did not consistently improve insertion, added retained row-template
memory to Prepared, and was removed. Full workspace tests passed the candidate;
a targeted repeated-insert/time test also passed. No peer win was established.
Background load is uncontrolled; these observations do not prove universal behavior.


</details>

### 52. 2026-10-01-map-position-ab

Артефакты: [2026-10-01-map-position-ab](../benches/results/2026-10-01-map-position-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-map-position-ab/report.md).

# Identity-map position reuse: rejected

Candidate retains existing moved-row ID/URI/SPO keys and updates positions, relying on column arrays that already followed swap_remove. Three alternating independent prebuilt process pairs, 24 fresh fixtures per case. Setup/destruction and affected-count/readback checks are outside timing.

| Pair | Rows | Baseline Lin µs | Candidate Lin µs | Baseline SQLite µs | Candidate SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | 1k | 7.854 | 7.562 | 2.896 | 4.000 |
| 1 | 10k | 13.875 | 14.521 | 9.812 | 12.521 |
| 1 | 100k | 23.896 | 19.709 | 16.750 | 15.959 |
| 2 | 1k | 6.521 | 7.438 | 3.208 | 4.604 |
| 2 | 10k | 13.521 | 15.104 | 10.042 | 10.687 |
| 2 | 100k | 21.354 | 20.062 | 16.834 | 15.938 |
| 3 | 1k | 6.250 | 5.084 | 3.583 | 3.604 |
| 3 | 10k | 13.771 | 14.459 | 10.396 | 12.625 |
| 3 | 100k | 20.645 | 21.541 | 17.438 | 17.729 |

Not retained: 1k and 100k won two pairs each, but 10k lost all three. SQLite is still faster in all measured matched deletion cases. Controls also fluctuate, so no causal regression percentage is claimed. Workspace tests and all sampled validators passed. The prior retained inline index/bound source was restored exactly; this report does not establish a universal peer win. Sources and raw observations remain.


</details>

### 53. 2026-10-01-merge-proposal

Артефакты: [2026-10-01-merge-proposal](../benches/results/2026-10-01-merge-proposal).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-merge-proposal/report.md).

# Proposed merge resolution with measured hybrid top-k

Date: 2026-10-02. Proposal is tested in an isolated archive of 8126c0c, NOT applied to the conflicted main checkout.

Keep the current verified embedding, FTS, slab-registration and bounded sparse WAL implementations. Preserve the branch's default Embedder.embed_batch_owned API without switching current inserts to it. Carry over hybrid bounded ranking and the large-bulk reopen regression test. Native bulk packing/large-batch persistence fixes are already covered by main's actual inserted-slice WAL path; keep its 16MiB record bound and sparse encoding rather than replacing it with the branch's 64MiB dense bound. Existing pending-posting and embedding bit compatibility paths remain.

Full offline workspace tests PASS; hybrid bounded-vs-full ranking checks three query texts and 42 skip/take combinations each, plus default top-50; includes ties, zero take and offsets beyond the last result. Large-bulk reopen regression passes. Initial test attempt used invalid search hybrid syntax and failed; fixed to the actual search syntax, then complete suite passed. Original failure log retained.

Six alternating process pairs, 24 observations each, one operation, warm prepared fts_hybrid_common case on 10k default documents. No tests/builds overlapped measurement. Both binaries preserve embedding, FTS and default behavior. Setup and input drop excluded; output drop included. Host load uncontrolled; no CPU-only, GPU or all-peer superiority claim.

Baseline median of process medians: 13.947979ms. Candidate: 8.289114ms. Median paired decrease: 40.44%; wins 6/6.

| Pair | Baseline ms | Candidate ms |
|---:|---:|---:|
| 1 | 13.030604 | 8.288291 |
| 2 | 13.081750 | 8.234500 |
| 3 | 14.367584 | 8.318812 |
| 4 | 21.752271 | 9.063396 |
| 5 | 15.007979 | 8.289937 |
| 6 | 13.528375 | 8.283313 |

The current merge still needs the user choice required by push-or-clear. No merge, push, branch deletion or worktree deletion performed. All-eight-peer objective remains unproven.


</details>

### 54. 2026-10-01-native-after-wal

Артефакты: [2026-10-01-native-after-wal](../benches/results/2026-10-01-native-after-wal).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-native-after-wal/report.md).

# Native insertion after retained WAL changes

Three independent process runs; 24 fresh fixtures per case, one operation each. Exact input timestamps and row validation unchanged. Native full Lin insert includes default hashing embeddings, FTS and CAS; competitor paths use their checked native bulk interfaces. Setup and teardown outside measurement. Pinned current production binary. Fixed case order and uncontrolled host load. Milliseconds.

| Case | Process 1 | Process 2 | Process 3 | Median of process medians |
|---|---:|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 1.031875 | 1.012500 | 1.001625 | 1.012500 |
| compare/insert_bulk_1k/sqlite | 0.789041 | 0.808480 | 0.787896 | 0.789041 |
| compare/insert_bulk_10k/lin | 10.664583 | 10.599083 | 10.646521 | 10.646521 |
| compare/insert_bulk_10k/sqlite | 10.353438 | 10.510562 | 10.451708 | 10.451708 |
| compare/insert_native_1k/duckdb_appender | 1.144729 | 1.178271 | 1.221958 | 1.178271 |
| compare/insert_native_10k/duckdb_appender | 10.205291 | 10.315188 | 10.448563 | 10.315188 |
| compare/insert_phase_10k/lin_full | 10.630042 | 11.710021 | 11.048958 | 11.048958 |
| compare/insert_phase_10k/lin_no_embed | 7.336562 | 8.173855 | 8.652104 | 8.173855 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index | 6.698562 | 6.624646 | 6.601771 | 6.624646 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5.216458 | 5.132062 | 5.216625 | 5.216458 |

Full native 10k Lin remains slower than SQLite and DuckDB Appender. Full native 1k beats Appender but loses SQLite. Phase cases disable named Lin components solely for diagnosis; they are not peer-equivalence or completion evidence. Differences across phases are not isolated causal timing proof.

PostgreSQL/MySQL unavailable to this native harness; explicitly excluded to avoid static-table helpers. MongoDB/pandas are covered by a separate peer harness, not these native write measurements. MSSQL/Kusto environment configuration is absent in this turn (presence booleans only; no secrets recorded). No overall superiority over all eight peers proven. Next experiment targets feature-hash modulo cost at default dimension, preserving bit-identical slot selection.


</details>

### 55. 2026-10-01-native-core

Артефакты: [2026-10-01-native-core](../benches/results/2026-10-01-native-core).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/append_log_10k/duckdb`, `compare/append_log_10k/lin`, `compare/append_log_10k/mysql`, `compare/append_log_10k/postgres`, `compare/append_log_10k/sqlite`, `compare/append_log_1k/duckdb`, `compare/append_log_1k/lin`, `compare/append_log_1k/mysql`, `compare/append_log_1k/postgres`, `compare/append_log_1k/sqlite`, `compare/filter_eq/duckdb`, `compare/filter_eq/lin`, `compare/filter_eq/mysql`, `compare/filter_eq/postgres`, `compare/filter_eq/sqlite`, `compare/filter_range/duckdb`, `compare/filter_range/lin`, `compare/filter_range/mysql`, `compare/filter_range/postgres`, `compare/filter_range/sqlite`, `compare/fts_hybrid_common/lin`, `compare/fts_lex_common/lin`, `compare/fts_lex_miss/lin`, `compare/fts_lex_selective/lin`, `compare/group_commit_16/lin_grouped_full`, `compare/group_commit_16/lin_sequential_full`, `compare/insert_bulk_10k/duckdb`, `compare/insert_bulk_10k/lin`, `compare/insert_bulk_10k/mysql`, `compare/insert_bulk_10k/postgres`, `compare/insert_bulk_10k/sqlite`, `compare/insert_bulk_1k/duckdb`, `compare/insert_bulk_1k/lin`, `compare/insert_bulk_1k/mysql`, `compare/insert_bulk_1k/postgres`, `compare/insert_bulk_1k/sqlite`, `compare/join_filter/duckdb`, `compare/join_filter/lin`, `compare/join_filter/lin_cursor`, `compare/join_filter/mysql`, `compare/join_filter/postgres`, `compare/join_filter/sqlite`, `compare/join_inner/duckdb`, `compare/join_inner/lin`, `compare/join_inner/lin_cursor`, `compare/join_inner/lin_cursor_projected`, `compare/join_inner/mysql`, `compare/join_inner/postgres`, `compare/join_inner/sqlite`, `compare/materialize/duckdb`, `compare/materialize/lin`, `compare/materialize/mysql`, `compare/materialize/postgres`, `compare/materialize/sqlite`, `compare/plan_dnf_10/lin`, `compare/plan_dnf_14/lin`, `compare/plan_dnf_6/lin`, `compare/point_get/duckdb`, `compare/point_get/hashmap`, `compare/point_get/lin`, `compare/point_get/mysql`, `compare/point_get/postgres`, `compare/point_get/sqlite`, `compare/text_substr/duckdb`, `compare/text_substr/lin`, `compare/text_substr/mysql`, `compare/text_substr/postgres`, `compare/text_substr/sqlite`.
- [progress.json](../benches/results/2026-10-01-native-core/progress.json)
- [run.json](../benches/results/2026-10-01-native-core/run.json)
- [status-final.json](../benches/results/2026-10-01-native-core/status-final.json)

### 56. 2026-10-01-native-profile-current

Артефакты: [2026-10-01-native-profile-current](../benches/results/2026-10-01-native-profile-current).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-native-profile-current/report.md).

# Current native bulk insert profile

Built the existing release profile_bulk example against current retained production changes. Sampled its main thread for 15 seconds at 1 ms. Workload completed 1305 fresh 10k-document batches in 20.012 seconds, with row count checks.

This is a sampling diagnostic, not a peer benchmark: it includes Db::empty, index setup, row-count validation and DB destruction; it uses the existing profile fixture with relative timestamps. Do not compare its total throughput to native benchmark timers.

The record_row call subtree is prominent (four direct stack groups: 1055, 890, 707 and 30 samples), with field/value allocation and BTree insertion beneath it. Embedding and FTS subtrees are also substantial. Raw sample.txt retained. Next experiment sorts insert record fields once during preparation and uses map bulk collection at execution, preserving duplicate last-value and runtime timestamps; evaluated in paired full native insert benchmarks separately.


</details>

### 57. 2026-10-01-norm-alternating

Артефакты: [2026-10-01-norm-alternating](../benches/results/2026-10-01-norm-alternating).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-norm-alternating/report.md).

# Sparse normalization comparison

Three independent process pairs; 12 fresh-fixture samples per case.
The second pair reverses variant order. All compilation finishes before measurement.
Exact variant sources and their SHA-256 hashes are included. Host CPU frequency and other load remain uncontrolled.
Full insertion improved in all three pairs; this is evidence for this fixture, not a general peer win.

| Process / variant | Full ms | No embedding ms | No embedding/index ms | No embedding/index/FTS ms |
|---|---:|---:|---:|---:|
| process-1-exact_sum | 13.064 | 9.372 | 8.228 | 5.865 |
| process-1-sorted | 14.393 | 9.694 | 8.627 | 6.150 |
| process-2-exact_sum | 13.131 | 9.463 | 8.319 | 6.007 |
| process-2-sorted | 14.131 | 9.513 | 8.376 | 5.960 |
| process-3-exact_sum | 13.362 | 9.449 | 8.329 | 6.048 |
| process-3-sorted | 14.102 | 9.750 | 8.338 | 5.954 |


</details>

### 58. 2026-10-01-pair-slots-ab

Артефакты: [2026-10-01-pair-slots-ab](../benches/results/2026-10-01-pair-slots-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-pair-slots-ab/report.md).

# Rejected per-batch byte-pair slot cache

Three independent process pairs, alternating order; 12 fresh fixtures per case.
The candidate allocated a 256 KiB lazy slot table for batches of at least 1024
texts. All byte pairs and several vector dimensions matched float bits, including
reuse and a large Unicode batch. Workspace tests passed.

| Pair | Original full 10k | Cached full 10k | Original no embed | Cached no embed |
|---|---:|---:|---:|---:|
| 1 | 24.435 ms | 30.080 ms | 14.914 ms | 14.015 ms |
| 2 | 12.257 ms | 15.419 ms | 9.480 ms | 10.705 ms |
| 3 | 14.320 ms | 16.193 ms | 10.639 ms | 10.845 ms |

The candidate lost all full-insertion pairs and was removed. Background CPU
load was substantial/uncontrolled and unchanged controls varied; no causal
regression percentage or peer win is claimed. Additional retained batch memory
is not justified by these observations. Sources, hashes and raw runs are saved.


</details>

### 59. 2026-10-01-pair-slots-before

Артефакты: [2026-10-01-pair-slots-before](../benches/results/2026-10-01-pair-slots-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-pair-slots-before/progress.json)
- [run.json](../benches/results/2026-10-01-pair-slots-before/run.json)
- [status-final.json](../benches/results/2026-10-01-pair-slots-before/status-final.json)

### 60. 2026-10-01-parallel-embed-ab

Артефакты: [2026-10-01-parallel-embed-ab](../benches/results/2026-10-01-parallel-embed-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-parallel-embed-ab/report.md).

# Rejected fresh-thread embedding experiment

Three independent process pairs, alternating order, 12 fresh fixtures per case.
Each >=4096-text batch creates up to four scoped workers; worker creation and
join are inside the measured insertion. Smaller/custom embedding paths unchanged.

| Pair | Serial full 10k | Scoped workers | Serial no embed | Worker-run no embed |
|---|---:|---:|---:|---:|
| 1 | 12.990 ms | 14.860 ms | 8.487 ms | 8.235 ms |
| 2 | 12.508 ms | 11.956 ms | 8.557 ms | 8.245 ms |
| 3 | 11.701 ms | 12.095 ms | 8.166 ms | 8.671 ms |

Two full-insertion pairs regressed; one improved. This implementation is
rejected. A separate reusable-worker-pool experiment follows. Workspace tests
and ordered bit-equivalence tests passed. Background load uncontrolled.
Sources/hashes/raw results are saved. No peer win is established here.


</details>

### 61. 2026-10-01-parallel-embed-before

Артефакты: [2026-10-01-parallel-embed-before](../benches/results/2026-10-01-parallel-embed-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-parallel-embed-before/progress.json)
- [run.json](../benches/results/2026-10-01-parallel-embed-before/run.json)
- [status-final.json](../benches/results/2026-10-01-parallel-embed-before/status-final.json)

### 62. 2026-10-01-peer-100k

Артефакты: [2026-10-01-peer-100k](../benches/results/2026-10-01-peer-100k).

<details>
<summary>process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-100k/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 1.032 | 4.17× | validated |
| point_get | duckdb | 40.372 | 163.13× | validated |
| point_get | postgres | 229.497 | 927.33× | validated |
| point_get | mysql | 276.396 | 1116.84× | validated |
| point_get | mongo | 391.441 | 1581.70× | validated |
| point_get | pandas | 4.604 | 18.60× | validated |
| filter_eq | lin | 0.791 | 1.00× | validated |
| filter_eq | sqlite | 714.118 | 903.20× | validated |
| filter_eq | duckdb | 428.335 | 541.75× | validated |
| filter_eq | postgres | 3147.854 | 3981.33× | validated |
| filter_eq | mysql | 3577.854 | 4525.18× | validated |
| filter_eq | mongo | 4163.354 | 5265.71× | validated |
| filter_eq | pandas | 2289.010 | 2895.08× | validated |
| text_substr | lin | 861.612 | 1.00× | validated |
| text_substr | sqlite | 3532.041 | 4.10× | validated |
| text_substr | duckdb | 482.681 | 0.56× | validated |
| text_substr | postgres | 5959.646 | 6.92× | validated |
| text_substr | mysql | 10195.062 | 11.83× | validated |
| text_substr | mongo | 24075.541 | 27.94× | validated |
| text_substr | pandas | 5030.750 | 5.84× | validated |
| materialize | lin | 6560.020 | 1.00× | validated |
| materialize | sqlite | 21097.312 | 3.22× | validated |
| materialize | duckdb | 12730.792 | 1.94× | validated |
| materialize | postgres | 15852.146 | 2.42× | validated |
| materialize | mysql | 86260.125 | 13.15× | validated |
| materialize | mongo | 46523.041 | 7.09× | validated |
| materialize | pandas | 18517.750 | 2.82× | validated |
| join_inner | lin | 14663.938 | 1.00× | validated |
| join_inner | sqlite | 57772.438 | 3.94× | validated |
| join_inner | duckdb | 29555.145 | 2.02× | validated |
| join_inner | postgres | 41862.126 | 2.85× | validated |
| join_inner | mysql | 177420.812 | 12.10× | validated |
| join_inner | mongo | 1158453.292 | 79.00× | validated |
| join_inner | pandas | 36443.105 | 2.49× | validated |
| join_filter | lin | 7686.626 | 1.00× | validated |
| join_filter | sqlite | 26567.791 | 3.46× | validated |
| join_filter | duckdb | 14681.416 | 1.91× | validated |
| join_filter | postgres | 22026.104 | 2.87× | validated |
| join_filter | mysql | 99965.541 | 13.01× | validated |
| join_filter | mongo | 597046.709 | 77.67× | validated |
| join_filter | pandas | 19104.416 | 2.49× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-100k/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.513 | 6.41× | validated |
| point_get | duckdb | 44.235 | 187.47× | validated |
| point_get | postgres | 323.528 | 1371.15× | validated |
| point_get | mysql | 363.771 | 1541.70× | validated |
| point_get | mongo | 363.400 | 1540.13× | validated |
| point_get | pandas | 4.731 | 20.05× | validated |
| point_get | lin | 0.236 | 1.00× | validated |
| filter_eq | sqlite | 1273.635 | 1646.62× | validated |
| filter_eq | duckdb | 450.562 | 582.51× | validated |
| filter_eq | postgres | 2940.312 | 3801.38× | validated |
| filter_eq | mysql | 3459.229 | 4472.26× | validated |
| filter_eq | mongo | 4290.708 | 5547.24× | validated |
| filter_eq | pandas | 2338.187 | 3022.93× | validated |
| filter_eq | lin | 0.773 | 1.00× | validated |
| text_substr | sqlite | 4461.417 | 5.22× | validated |
| text_substr | duckdb | 488.704 | 0.57× | validated |
| text_substr | postgres | 5813.646 | 6.80× | validated |
| text_substr | mysql | 9781.646 | 11.43× | validated |
| text_substr | mongo | 23872.500 | 27.91× | validated |
| text_substr | pandas | 5227.645 | 6.11× | validated |
| text_substr | lin | 855.437 | 1.00× | validated |
| materialize | sqlite | 21355.959 | 3.36× | validated |
| materialize | duckdb | 12575.354 | 1.98× | validated |
| materialize | postgres | 15231.854 | 2.39× | validated |
| materialize | mysql | 89876.417 | 14.13× | validated |
| materialize | mongo | 45948.479 | 7.22× | validated |
| materialize | pandas | 21098.000 | 3.32× | validated |
| materialize | lin | 6361.042 | 1.00× | validated |
| join_inner | sqlite | 52727.166 | 3.57× | validated |
| join_inner | duckdb | 29379.479 | 1.99× | validated |
| join_inner | postgres | 41336.626 | 2.80× | validated |
| join_inner | mysql | 177279.771 | 12.02× | validated |
| join_inner | mongo | 1212882.229 | 82.23× | validated |
| join_inner | pandas | 37337.396 | 2.53× | validated |
| join_inner | lin | 14750.667 | 1.00× | validated |
| join_filter | sqlite | 26004.625 | 3.58× | validated |
| join_filter | duckdb | 14352.792 | 1.97× | validated |
| join_filter | postgres | 22274.188 | 3.06× | validated |
| join_filter | mysql | 94298.625 | 12.97× | validated |
| join_filter | mongo | 597402.688 | 82.17× | validated |
| join_filter | pandas | 19291.834 | 2.65× | validated |
| join_filter | lin | 7270.562 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-100k/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 43.834 | 185.73× | validated |
| point_get | postgres | 295.713 | 1252.96× | validated |
| point_get | mysql | 360.752 | 1528.53× | validated |
| point_get | mongo | 1320.174 | 5593.66× | validated |
| point_get | pandas | 4.673 | 19.80× | validated |
| point_get | lin | 0.236 | 1.00× | validated |
| point_get | sqlite | 1.034 | 4.38× | validated |
| filter_eq | duckdb | 453.389 | 581.58× | validated |
| filter_eq | postgres | 2636.354 | 3381.78× | validated |
| filter_eq | mysql | 3674.854 | 4713.91× | validated |
| filter_eq | mongo | 6572.958 | 8431.45× | validated |
| filter_eq | pandas | 2337.635 | 2998.60× | validated |
| filter_eq | lin | 0.780 | 1.00× | validated |
| filter_eq | sqlite | 719.309 | 922.69× | validated |
| text_substr | duckdb | 490.748 | 0.58× | validated |
| text_substr | postgres | 5571.042 | 6.55× | validated |
| text_substr | mysql | 10051.729 | 11.82× | validated |
| text_substr | mongo | 25801.750 | 30.35× | validated |
| text_substr | pandas | 4959.104 | 5.83× | validated |
| text_substr | lin | 850.274 | 1.00× | validated |
| text_substr | sqlite | 3576.313 | 4.21× | validated |
| materialize | duckdb | 12785.458 | 2.17× | validated |
| materialize | postgres | 18511.959 | 3.14× | validated |
| materialize | mysql | 87048.500 | 14.76× | validated |
| materialize | mongo | 46197.105 | 7.83× | validated |
| materialize | pandas | 17969.000 | 3.05× | validated |
| materialize | lin | 5898.417 | 1.00× | validated |
| materialize | sqlite | 21732.834 | 3.68× | validated |
| join_inner | duckdb | 30088.479 | 2.07× | validated |
| join_inner | postgres | 46402.271 | 3.20× | validated |
| join_inner | mysql | 180511.791 | 12.45× | validated |
| join_inner | mongo | 1184294.374 | 81.67× | validated |
| join_inner | pandas | 37456.812 | 2.58× | validated |
| join_inner | lin | 14500.854 | 1.00× | validated |
| join_inner | sqlite | 53347.709 | 3.68× | validated |
| join_filter | duckdb | 14067.750 | 1.86× | validated |
| join_filter | postgres | 22405.396 | 2.96× | validated |
| join_filter | mysql | 95286.917 | 12.60× | validated |
| join_filter | mongo | 606315.938 | 80.21× | validated |
| join_filter | pandas | 19384.730 | 2.56× | validated |
| join_filter | lin | 7559.542 | 1.00× | validated |
| join_filter | sqlite | 27307.730 | 3.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-100k/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.236 | 1.00× | validated |
| point_get | sqlite | 1.034 | 4.38× | validated |
| point_get | duckdb | 43.834 | 185.73× | validated |
| point_get | postgres | 295.713 | 1252.96× | validated |
| point_get | mysql | 360.752 | 1528.53× | validated |
| point_get | mongo | 391.441 | 1658.56× | validated |
| point_get | pandas | 4.673 | 19.80× | validated |
| filter_eq | lin | 0.780 | 1.00× | validated |
| filter_eq | sqlite | 719.309 | 922.69× | validated |
| filter_eq | duckdb | 450.562 | 577.96× | validated |
| filter_eq | postgres | 2940.312 | 3771.68× | validated |
| filter_eq | mysql | 3577.854 | 4589.48× | validated |
| filter_eq | mongo | 4290.708 | 5503.90× | validated |
| filter_eq | pandas | 2337.635 | 2998.60× | validated |
| text_substr | lin | 855.437 | 1.00× | validated |
| text_substr | sqlite | 3576.313 | 4.18× | validated |
| text_substr | duckdb | 488.704 | 0.57× | validated |
| text_substr | postgres | 5813.646 | 6.80× | validated |
| text_substr | mysql | 10051.729 | 11.75× | validated |
| text_substr | mongo | 24075.541 | 28.14× | validated |
| text_substr | pandas | 5030.750 | 5.88× | validated |
| materialize | lin | 6361.042 | 1.00× | validated |
| materialize | sqlite | 21355.959 | 3.36× | validated |
| materialize | duckdb | 12730.792 | 2.00× | validated |
| materialize | postgres | 15852.146 | 2.49× | validated |
| materialize | mysql | 87048.500 | 13.68× | validated |
| materialize | mongo | 46197.105 | 7.26× | validated |
| materialize | pandas | 18517.750 | 2.91× | validated |
| join_inner | lin | 14663.938 | 1.00× | validated |
| join_inner | sqlite | 53347.709 | 3.64× | validated |
| join_inner | duckdb | 29555.145 | 2.02× | validated |
| join_inner | postgres | 41862.126 | 2.85× | validated |
| join_inner | mysql | 177420.812 | 12.10× | validated |
| join_inner | mongo | 1184294.374 | 80.76× | validated |
| join_inner | pandas | 37337.396 | 2.55× | validated |
| join_filter | lin | 7559.542 | 1.00× | validated |
| join_filter | sqlite | 26567.791 | 3.51× | validated |
| join_filter | duckdb | 14352.792 | 1.90× | validated |
| join_filter | postgres | 22274.188 | 2.95× | validated |
| join_filter | mysql | 95286.917 | 12.60× | validated |
| join_filter | mongo | 597402.688 | 79.03× | validated |
| join_filter | pandas | 19291.834 | 2.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

### 63. 2026-10-01-peer-10k

Артефакты: [2026-10-01-peer-10k](../benches/results/2026-10-01-peer-10k).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-10k/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.018 | 4.15× | validated |
| point_get | duckdb | 61.767 | 251.52× | validated |
| point_get | postgres | 550.058 | 2239.90× | validated |
| point_get | mysql | 331.253 | 1348.90× | validated |
| point_get | mongo | 576.513 | 2347.62× | validated |
| point_get | pandas | 4.707 | 19.17× | validated |
| filter_eq | lin | 0.775 | 1.00× | validated |
| filter_eq | sqlite | 78.198 | 100.95× | validated |
| filter_eq | duckdb | 271.081 | 349.95× | validated |
| filter_eq | postgres | 873.839 | 1128.08× | validated |
| filter_eq | mysql | 773.569 | 998.64× | validated |
| filter_eq | mongo | 1374.062 | 1773.84× | validated |
| filter_eq | pandas | 273.132 | 352.60× | validated |
| text_substr | lin | 83.547 | 1.00× | validated |
| text_substr | sqlite | 374.995 | 4.49× | validated |
| text_substr | duckdb | 117.022 | 1.40× | validated |
| text_substr | postgres | 1385.438 | 16.58× | validated |
| text_substr | mysql | 2228.958 | 26.68× | validated |
| text_substr | mongo | 4065.396 | 48.66× | validated |
| text_substr | pandas | 509.968 | 6.10× | validated |
| materialize | lin | 425.625 | 1.00× | validated |
| materialize | sqlite | 2187.854 | 5.14× | validated |
| materialize | duckdb | 1778.052 | 4.18× | validated |
| materialize | postgres | 2863.146 | 6.73× | validated |
| materialize | mysql | 13446.458 | 31.59× | validated |
| materialize | mongo | 6877.979 | 16.16× | validated |
| materialize | pandas | 1925.750 | 4.52× | validated |
| join_inner | lin | 1379.090 | 1.00× | validated |
| join_inner | sqlite | 5190.730 | 3.76× | validated |
| join_inner | duckdb | 3761.416 | 2.73× | validated |
| join_inner | postgres | 5504.917 | 3.99× | validated |
| join_inner | mysql | 26859.271 | 19.48× | validated |
| join_inner | mongo | 145461.062 | 105.48× | validated |
| join_inner | pandas | 4177.229 | 3.03× | validated |
| join_filter | lin | 676.390 | 1.00× | validated |
| join_filter | sqlite | 2649.834 | 3.92× | validated |
| join_filter | duckdb | 2153.552 | 3.18× | validated |
| join_filter | postgres | 3253.062 | 4.81× | validated |
| join_filter | mysql | 13119.646 | 19.40× | validated |
| join_filter | mongo | 59697.854 | 88.26× | validated |
| join_filter | pandas | 2484.167 | 3.67× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

### 64. 2026-10-01-peer-current-100k

Артефакты: [2026-10-01-peer-current-100k](../benches/results/2026-10-01-peer-current-100k).

<details>
<summary>process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-current-100k/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.250 | 1.00× | validated |
| point_get | sqlite | 1.033 | 4.13× | validated |
| point_get | duckdb | 49.434 | 197.62× | validated |
| point_get | postgres | 265.323 | 1060.66× | validated |
| point_get | mysql | 360.799 | 1442.33× | validated |
| point_get | mongo | 467.123 | 1867.37× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.950 | 19.79× | validated |
| filter_eq | lin | 0.797 | 1.00× | validated |
| filter_eq | sqlite | 704.006 | 883.43× | validated |
| filter_eq | duckdb | 439.099 | 551.01× | validated |
| filter_eq | postgres | 3432.681 | 4307.56× | validated |
| filter_eq | mysql | 3592.916 | 4508.63× | validated |
| filter_eq | mongo | 4235.521 | 5315.02× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2446.000 | 3069.41× | validated |
| text_substr | lin | 207.823 | 1.00× | validated |
| text_substr | sqlite | 3550.750 | 17.09× | validated |
| text_substr | duckdb | 534.971 | 2.57× | validated |
| text_substr | postgres | 6485.125 | 31.20× | validated |
| text_substr | mysql | 10002.584 | 48.13× | validated |
| text_substr | mongo | 23774.959 | 114.40× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5600.917 | 26.95× | validated |
| materialize | lin | 5695.917 | 1.00× | validated |
| materialize | sqlite | 21243.917 | 3.73× | validated |
| materialize | duckdb | 15795.958 | 2.77× | validated |
| materialize | postgres | 14987.000 | 2.63× | validated |
| materialize | mysql | 86868.125 | 15.25× | validated |
| materialize | mongo | 45423.708 | 7.97× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 19253.542 | 3.38× | validated |
| join_inner | lin | 14248.959 | 1.00× | validated |
| join_inner | sqlite | 52149.416 | 3.66× | validated |
| join_inner | duckdb | 29825.042 | 2.09× | validated |
| join_inner | postgres | 40611.833 | 2.85× | validated |
| join_inner | mysql | 175780.458 | 12.34× | validated |
| join_inner | mongo | 1154165.417 | 81.00× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 71675.250 | 5.03× | validated |
| join_filter | lin | 7265.625 | 1.00× | validated |
| join_filter | sqlite | 26300.167 | 3.62× | validated |
| join_filter | duckdb | 16029.416 | 2.21× | validated |
| join_filter | postgres | 22260.000 | 3.06× | validated |
| join_filter | mysql | 93758.666 | 12.90× | validated |
| join_filter | mongo | 585696.583 | 80.61× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 20554.458 | 2.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-current-100k/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.038 | 2.79× | validated |
| point_get | duckdb | 74.307 | 199.48× | validated |
| point_get | postgres | 245.314 | 658.55× | validated |
| point_get | mysql | 943.052 | 2531.65× | validated |
| point_get | mongo | 915.833 | 2458.59× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.755 | 18.13× | validated |
| point_get | lin | 0.373 | 1.00× | validated |
| filter_eq | sqlite | 699.030 | 419.64× | validated |
| filter_eq | duckdb | 587.156 | 352.48× | validated |
| filter_eq | postgres | 3341.667 | 2006.07× | validated |
| filter_eq | mysql | 5189.770 | 3115.53× | validated |
| filter_eq | mongo | 5874.791 | 3526.76× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 4338.625 | 2604.57× | validated |
| filter_eq | lin | 1.666 | 1.00× | validated |
| text_substr | sqlite | 3578.584 | 9.84× | validated |
| text_substr | duckdb | 1525.396 | 4.20× | validated |
| text_substr | postgres | 6405.041 | 17.62× | validated |
| text_substr | mysql | 13330.000 | 36.67× | validated |
| text_substr | mongo | 30695.125 | 84.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 11484.458 | 31.59× | validated |
| text_substr | lin | 363.554 | 1.00× | validated |
| materialize | sqlite | 21960.792 | 1.31× | validated |
| materialize | duckdb | 49900.625 | 2.99× | validated |
| materialize | postgres | 19632.167 | 1.17× | validated |
| materialize | mysql | 126514.125 | 7.57× | validated |
| materialize | mongo | 66817.958 | 4.00× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 38948.875 | 2.33× | validated |
| materialize | lin | 16710.750 | 1.00× | validated |
| join_inner | sqlite | 84809.875 | 2.29× | validated |
| join_inner | duckdb | 64739.459 | 1.75× | validated |
| join_inner | postgres | 46244.709 | 1.25× | validated |
| join_inner | mysql | 477177.792 | 12.88× | validated |
| join_inner | mongo | 1907699.334 | 51.48× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 197123.583 | 5.32× | validated |
| join_inner | lin | 37059.834 | 1.00× | validated |
| join_filter | sqlite | 64042.250 | 3.59× | validated |
| join_filter | duckdb | 20762.959 | 1.16× | validated |
| join_filter | postgres | 28539.500 | 1.60× | validated |
| join_filter | mysql | 265309.958 | 14.87× | validated |
| join_filter | mongo | 1026089.917 | 57.50× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 125420.584 | 7.03× | validated |
| join_filter | lin | 17844.292 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-current-100k/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.713 | 168.15× | validated |
| point_get | postgres | 276.692 | 1142.76× | validated |
| point_get | mysql | 344.296 | 1421.98× | validated |
| point_get | mongo | 468.449 | 1934.74× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.785 | 19.76× | validated |
| point_get | lin | 0.242 | 1.00× | validated |
| point_get | sqlite | 1.055 | 4.36× | validated |
| filter_eq | duckdb | 479.454 | 604.32× | validated |
| filter_eq | postgres | 2859.861 | 3604.66× | validated |
| filter_eq | mysql | 3970.000 | 5003.91× | validated |
| filter_eq | mongo | 4232.625 | 5334.93× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2306.010 | 2906.57× | validated |
| filter_eq | lin | 0.793 | 1.00× | validated |
| filter_eq | sqlite | 693.475 | 874.08× | validated |
| text_substr | duckdb | 516.357 | 2.49× | validated |
| text_substr | postgres | 6339.917 | 30.61× | validated |
| text_substr | mysql | 10641.041 | 51.37× | validated |
| text_substr | mongo | 24116.417 | 116.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4946.625 | 23.88× | validated |
| text_substr | lin | 207.125 | 1.00× | validated |
| text_substr | sqlite | 3530.521 | 17.05× | validated |
| materialize | duckdb | 13258.291 | 2.20× | validated |
| materialize | postgres | 16232.959 | 2.69× | validated |
| materialize | mysql | 91822.292 | 15.23× | validated |
| materialize | mongo | 48193.125 | 7.99× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 18288.750 | 3.03× | validated |
| materialize | lin | 6028.625 | 1.00× | validated |
| materialize | sqlite | 22053.833 | 3.66× | validated |
| join_inner | duckdb | 29760.167 | 2.07× | validated |
| join_inner | postgres | 42348.583 | 2.94× | validated |
| join_inner | mysql | 178857.708 | 12.42× | validated |
| join_inner | mongo | 1163426.917 | 80.80× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 37004.625 | 2.57× | validated |
| join_inner | lin | 14398.667 | 1.00× | validated |
| join_inner | sqlite | 52585.208 | 3.65× | validated |
| join_filter | duckdb | 14657.042 | 2.03× | validated |
| join_filter | postgres | 22232.000 | 3.07× | validated |
| join_filter | mysql | 103737.958 | 14.34× | validated |
| join_filter | mongo | 615641.375 | 85.11× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19264.625 | 2.66× | validated |
| join_filter | lin | 7233.333 | 1.00× | validated |
| join_filter | sqlite | 27394.541 | 3.79× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-peer-current-100k/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.250 | 1.00× | validated |
| point_get | sqlite | 1.038 | 4.15× | validated |
| point_get | duckdb | 49.434 | 197.62× | validated |
| point_get | postgres | 265.323 | 1060.66× | validated |
| point_get | mysql | 360.799 | 1442.33× | validated |
| point_get | mongo | 468.449 | 1872.67× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.950 | 19.79× | validated |
| filter_eq | lin | 0.797 | 1.00× | validated |
| filter_eq | sqlite | 699.030 | 877.19× | validated |
| filter_eq | duckdb | 479.454 | 601.65× | validated |
| filter_eq | postgres | 3341.667 | 4193.35× | validated |
| filter_eq | mysql | 3970.000 | 4981.82× | validated |
| filter_eq | mongo | 4235.521 | 5315.02× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2446.000 | 3069.41× | validated |
| text_substr | lin | 207.823 | 1.00× | validated |
| text_substr | sqlite | 3550.750 | 17.09× | validated |
| text_substr | duckdb | 534.971 | 2.57× | validated |
| text_substr | postgres | 6405.041 | 30.82× | validated |
| text_substr | mysql | 10641.041 | 51.20× | validated |
| text_substr | mongo | 24116.417 | 116.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5600.917 | 26.95× | validated |
| materialize | lin | 6028.625 | 1.00× | validated |
| materialize | sqlite | 21960.792 | 3.64× | validated |
| materialize | duckdb | 15795.958 | 2.62× | validated |
| materialize | postgres | 16232.959 | 2.69× | validated |
| materialize | mysql | 91822.292 | 15.23× | validated |
| materialize | mongo | 48193.125 | 7.99× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 19253.542 | 3.19× | validated |
| join_inner | lin | 14398.667 | 1.00× | validated |
| join_inner | sqlite | 52585.208 | 3.65× | validated |
| join_inner | duckdb | 29825.042 | 2.07× | validated |
| join_inner | postgres | 42348.583 | 2.94× | validated |
| join_inner | mysql | 178857.708 | 12.42× | validated |
| join_inner | mongo | 1163426.917 | 80.80× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 71675.250 | 4.98× | validated |
| join_filter | lin | 7265.625 | 1.00× | validated |
| join_filter | sqlite | 27394.541 | 3.77× | validated |
| join_filter | duckdb | 16029.416 | 2.21× | validated |
| join_filter | postgres | 22260.000 | 3.06× | validated |
| join_filter | mysql | 103737.958 | 14.28× | validated |
| join_filter | mongo | 615641.375 | 84.73× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 20554.458 | 2.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>summary.md</summary>

Источник: [summary.md](../benches/results/2026-10-01-peer-current-100k/summary.md).

# Current read benchmark summary

100k rows; three independent Python processes, rotated engine order, nine samples.
36/36 measured read comparisons favor Lin. 12/48 required comparisons are missing
because MSSQL and Kusto have no dedicated endpoint configured. Overall status: incomplete.

| Case | Lin µs | DuckDB µs | SQLite µs |
|---|---:|---:|---:|
| point_get | 0.250 | 49.434 | 1.038 |
| filter_eq | 0.797 | 479.454 | 699.030 |
| text_substr | 207.823 | 534.971 | 3550.750 |
| materialize | 6028.625 | 15795.958 | 21960.792 |
| join_inner | 14398.667 | 29825.042 | 52585.208 |
| join_filter | 7265.625 | 16029.416 | 27394.541 |

Exact values and duplicate multiplicities validated outside timing. Lin uses the
Rust prepared API; peers use Python DBAPI/DataFrame/driver APIs. Server queries
include network round-trip and result transfer. This is not isolated engine CPU
comparison. Background load is uncontrolled.

Current 10k ingestion still loses: Lin 14.636 ms vs SQLite 10.621 ms and DuckDB
Appender 10.480 ms in the separate native Rust run (bulk-fts-append). Default
Lin embedding/FTS and plain SQL fixture schemas perform different work.

[All peer results](../benches/results/2026-10-01-peer-current-100k/report.md) and completion-audit.json retain missing-peer status.


</details>

### 65. 2026-10-01-pg-owned-schema

Артефакты: [2026-10-01-pg-owned-schema](../benches/results/2026-10-01-pg-owned-schema).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-pg-owned-schema/report.md).

# PostgreSQL native benchmark fixture isolation

Native helpers previously dropped fixed names in the default schema. Each PostgreSQL fixture now creates one uniquely named persistent schema, sets search_path to it, and removes the owned schema when the fixture client drops. Warm reads, joins, bulk docs and logs share this ownership wrapper. Ordinary WAL-logged tables, transaction semantics and benchmark timer boundaries retained. Requires CREATE privilege on the test database. Cleanup failures are reported.

Added LIN_BENCH_SKIP_MYSQL to suppress MySQL connection/setup during independent PostgreSQL runs. MySQL isolation remains outstanding; do not infer this change makes that helper safe. The separate Python peer harness already uses owned table names.

## Live verification

Compiled the native benchmark offline. Started only the pre-existing owned lin-bench-postgres-1 container, created a unique validation database, and created five permanent sentinel tables in public: docs, docs_bulk, users, orders, logs_bulk. Ran all 11 PostgreSQL cases with one sample/operation each. Every case completed. All five sentinel values remained identical; zero lin_bench_* schemas remained. Exact evidence saved in verification.json and validation/run.json. Validation database removed and owned container stopped afterward.

This run verifies isolation and execution coverage, not performance superiority, timing stability, or full peer write equivalence. Real performance runs still required; native PostgreSQL bulk/log readback validation remains to strengthen. No complete result for MSSQL/Kusto yet. The full all-eight-peer goal remains active and unproven. Source and binary hashes saved.


</details>

### 66. 2026-10-01-pg-validated-insert

Артефакты: [2026-10-01-pg-validated-insert](../benches/results/2026-10-01-pg-validated-insert).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-pg-validated-insert/report.md).

# Validated PostgreSQL native insert and binary COPY

PostgreSQL rowwise transactional insert now uses bench_checked with complete committed readback of id, uri, wing, title, exact timestamp and body. Validation opens a separate connection and reads the owned fixture schema, outside the timer. New binary COPY cases preserve the same table/index constraints and explicit transaction commit; COPY row count checked and all fields read back. This avoids presenting rowwise network round trips as PostgreSQL bulk capability.

Three independent processes, eight fresh fixtures per case, one operation each. Setup/prepare and fixture cleanup excluded; actual write/commit included. Default Lin embeddings, FTS and CAS remain enabled. Fixed case order, uncontrolled host load; no significance claim. Milliseconds.

| Case | Process 1 | Process 2 | Process 3 | Median of process medians |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 0.990771 | 0.925437 | 0.962959 | 0.962959 |
| compare/insert_bulk_1k/sqlite | 0.806271 | 0.840625 | 0.828105 | 0.828105 |
| compare/insert_bulk_1k/postgres | 225.301604 | 221.363479 | 234.260938 | 225.301604 |
| compare/insert_native_1k/postgres_copy | 4.328750 | 5.365105 | 4.715834 | 4.715834 |
| compare/insert_bulk_10k/lin | 10.257459 | 11.144000 | 10.101458 | 10.257459 |
| compare/insert_bulk_10k/sqlite | 10.679541 | 11.631626 | 10.586813 | 10.679541 |
| compare/insert_bulk_10k/postgres | 1974.377104 | 2510.812416 | 2028.728125 | 2028.728125 |
| compare/insert_native_10k/postgres_copy | 28.298229 | 30.476604 | 28.884771 | 28.884771 |
| compare/insert_native_1k/duckdb_appender | 1.300666 | 1.301896 | 1.436562 | 1.301896 |
| compare/insert_native_10k/duckdb_appender | 10.415125 | 10.501646 | 10.729833 | 10.501646 |

Lin beats tested PostgreSQL COPY at both sizes in all three processes. Lin 10k beats SQLite 3/3 and DuckDB Appender 2/3; 1k loses SQLite 3/3 and beats Appender 3/3. These scoped host/driver comparisons do not prove universal engine superiority or the all-eight-peer objective.

## Environment and cleanup

PostgreSQL 16.15; synchronous_commit=on, fsync=on, full_page_writes=on, wal_level=replica. Data directory is tmpfs under the existing Docker compose configuration, so this is not physical-disk/power-loss durability evidence. Native Lin/SQLite/DuckDB inputs are in memory; PostgreSQL still performs normal WAL transactions and driver/server round trips.

All runs completed and validation passed. Zero owned schemas remained after every process. Unique test database removed and pre-existing owned PostgreSQL container stopped afterward. verification.json records completion and binary hash. Offline native benchmark compile, scoped rustfmt and diff checks pass.

MySQL native helper still requires safe isolation; MSSQL/Kusto endpoints unavailable, other peer coverage not refreshed in this run. Goal remains active and unproven. Raw observations/logs and source hashes retained.


</details>

### 67. 2026-10-01-posting-tail-ab

Артефакты: [2026-10-01-posting-tail-ab](../benches/results/2026-10-01-posting-tail-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-posting-tail-ab/report.md).

# Posting-tail experiment and corrected timestamp fixtures

## Retained change: benchmark input parity

Warm Lin fixtures used `ago 1d`/`ago 30d` independently for each 500-row seed chunk. SQLite received the two absolute timestamps from Doc. Lin therefore could have many distinct [wing, ts] keys instead of the two used by SQLite; elapsed seeding time changed the index shape. Earlier warm-fixture peer timings using that loader do not establish exact timestamp/key-distribution parity. This does not identify a code regression or invalidate unrelated harnesses, but those earlier numbers cannot prove the current corrected native-write comparison.

The retained bench change anchors Doc timestamps once per benchmark process for all engines/fixtures. After Lin warm fixture loading, exact Cell::Time values are assigned from the same Doc records and scalar indexes are rebuilt outside timing. Every ID/timestamp pair and exactly two composite keys are checked before measurement. This only changes prepared read/write fixture setup; timed native ingestion still uses the public relative-time syntax and has not gained exact timestamp-value parity from this change. No optimization feature was disabled.

## Index candidates: not retained

The experiment checks the posting tail first, then uses the original linear search when necessary. The broad variant used this in both removal and key replacement. The narrow variant only changed remove_at, keeping insert_key unchanged. Unsorted unique posting lists have the same final vector/order as the original swap_remove algorithm; a new oracle test exercised empty/missing/tail/interior removal on shuffled lists through length 4096. The complete snapshot default workspace suite passed. Source variants are kept as experimental artifacts; production src/index.rs remains the original.

Fresh 100k deletion improved after input normalization, but smaller cases and steady-state writes did not give a consistent overall win. Narrow steady-state churn lost 2/3 pairs, so it was rejected. Broad normalized updates and small deletes were also mixed/negative. We do not claim these timing differences are isolated causal regressions: SQLite controls and shared host load varied too. No statistical significance is established.

## Corrected fixtures: broad candidate

Three independent prebuilt process pairs, alternating order; 24 fresh fixtures, one operation each. Setup/destruction and affected-row/readback checks are outside timing. Compiler/tests completed before measurements.

| Pair | Case | Baseline µs | Candidate µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 3.188 | 3.854 | -20.9% |
| 1 | update_1row_1k/sqlite | 4.146 | 6.333 | -52.8% |
| 1 | delete_1row_1k/lin | 8.916 | 10.083 | -13.1% |
| 1 | delete_1row_1k/sqlite | 4.688 | 4.562 | 2.7% |
| 1 | update_1row_10k/lin | 8.792 | 9.854 | -12.1% |
| 1 | update_1row_10k/sqlite | 12.812 | 12.396 | 3.3% |
| 1 | delete_1row_10k/lin | 16.229 | 16.854 | -3.9% |
| 1 | delete_1row_10k/sqlite | 10.146 | 13.937 | -37.4% |
| 1 | update_1row_100k/lin | 12.396 | 14.209 | -14.6% |
| 1 | update_1row_100k/sqlite | 16.916 | 19.104 | -12.9% |
| 1 | delete_1row_100k/lin | 36.605 | 21.375 | 41.6% |
| 1 | delete_1row_100k/sqlite | 19.999 | 18.584 | 7.1% |
| 2 | update_1row_1k/lin | 3.833 | 4.479 | -16.8% |
| 2 | update_1row_1k/sqlite | 3.521 | 3.916 | -11.2% |
| 2 | delete_1row_1k/lin | 4.875 | 5.833 | -19.7% |
| 2 | delete_1row_1k/sqlite | 3.417 | 3.750 | -9.7% |
| 2 | update_1row_10k/lin | 8.688 | 8.167 | 6.0% |
| 2 | update_1row_10k/sqlite | 9.666 | 11.917 | -23.3% |
| 2 | delete_1row_10k/lin | 14.812 | 14.334 | 3.2% |
| 2 | delete_1row_10k/sqlite | 9.959 | 12.062 | -21.1% |
| 2 | update_1row_100k/lin | 12.021 | 12.959 | -7.8% |
| 2 | update_1row_100k/sqlite | 17.146 | 18.188 | -6.1% |
| 2 | delete_1row_100k/lin | 34.896 | 22.709 | 34.9% |
| 2 | delete_1row_100k/sqlite | 17.729 | 16.646 | 6.1% |
| 3 | update_1row_1k/lin | 4.479 | 5.292 | -18.2% |
| 3 | update_1row_1k/sqlite | 5.062 | 5.688 | -12.4% |
| 3 | delete_1row_1k/lin | 7.125 | 6.396 | 10.2% |
| 3 | delete_1row_1k/sqlite | 4.521 | 5.250 | -16.1% |
| 3 | update_1row_10k/lin | 9.062 | 9.271 | -2.3% |
| 3 | update_1row_10k/sqlite | 10.458 | 13.229 | -26.5% |
| 3 | delete_1row_10k/lin | 15.146 | 16.458 | -8.7% |
| 3 | delete_1row_10k/sqlite | 10.875 | 12.666 | -16.5% |
| 3 | update_1row_100k/lin | 12.125 | 14.979 | -23.5% |
| 3 | update_1row_100k/sqlite | 16.333 | 19.938 | -22.1% |
| 3 | delete_1row_100k/lin | 34.062 | 24.750 | 27.3% |
| 3 | delete_1row_100k/sqlite | 18.730 | 19.750 | -5.4% |

## Corrected fixtures: removal-only candidate

Same fixture/timer contract; process order was reversed relative to the broad cohort.

| Pair | Case | Baseline µs | Candidate µs | Reduction |
|---|---|---:|---:|---:|
| 1 | update_1row_1k/lin | 4.354 | 6.229 | -43.1% |
| 1 | update_1row_1k/sqlite | 3.771 | 6.083 | -61.3% |
| 1 | delete_1row_1k/lin | 8.625 | 10.562 | -22.5% |
| 1 | delete_1row_1k/sqlite | 7.792 | 7.062 | 9.4% |
| 1 | update_1row_10k/lin | 8.688 | 11.041 | -27.1% |
| 1 | update_1row_10k/sqlite | 8.479 | 13.562 | -59.9% |
| 1 | delete_1row_10k/lin | 14.917 | 17.000 | -14.0% |
| 1 | delete_1row_10k/sqlite | 10.062 | 12.042 | -19.7% |
| 1 | update_1row_100k/lin | 15.167 | 18.521 | -22.1% |
| 1 | update_1row_100k/sqlite | 20.458 | 21.374 | -4.5% |
| 1 | delete_1row_100k/lin | 39.041 | 26.542 | 32.0% |
| 1 | delete_1row_100k/sqlite | 18.959 | 19.458 | -2.6% |
| 2 | update_1row_1k/lin | 6.417 | 5.604 | 12.7% |
| 2 | update_1row_1k/sqlite | 7.687 | 5.896 | 23.3% |
| 2 | delete_1row_1k/lin | 7.417 | 7.250 | 2.2% |
| 2 | delete_1row_1k/sqlite | 5.042 | 4.771 | 5.4% |
| 2 | update_1row_10k/lin | 9.562 | 9.500 | 0.6% |
| 2 | update_1row_10k/sqlite | 12.500 | 12.521 | -0.2% |
| 2 | delete_1row_10k/lin | 17.792 | 15.146 | 14.9% |
| 2 | delete_1row_10k/sqlite | 13.771 | 11.291 | 18.0% |
| 2 | update_1row_100k/lin | 14.479 | 13.167 | 9.1% |
| 2 | update_1row_100k/sqlite | 20.688 | 16.688 | 19.3% |
| 2 | delete_1row_100k/lin | 41.478 | 24.791 | 40.2% |
| 2 | delete_1row_100k/sqlite | 19.666 | 14.541 | 26.1% |
| 3 | update_1row_1k/lin | 5.812 | 4.146 | 28.7% |
| 3 | update_1row_1k/sqlite | 5.896 | 3.583 | 39.2% |
| 3 | delete_1row_1k/lin | 8.041 | 9.292 | -15.6% |
| 3 | delete_1row_1k/sqlite | 3.896 | 3.646 | 6.4% |
| 3 | update_1row_10k/lin | 10.521 | 9.834 | 6.5% |
| 3 | update_1row_10k/sqlite | 12.875 | 11.708 | 9.1% |
| 3 | delete_1row_10k/lin | 27.062 | 13.791 | 49.0% |
| 3 | delete_1row_10k/sqlite | 17.291 | 9.584 | 44.6% |
| 3 | update_1row_100k/lin | 16.166 | 12.916 | 20.1% |
| 3 | update_1row_100k/sqlite | 19.875 | 14.479 | 27.1% |
| 3 | delete_1row_100k/lin | 40.729 | 21.541 | 47.1% |
| 3 | delete_1row_100k/sqlite | 18.188 | 15.521 | 14.7% |

## Steady-state diagnostics

10k rows, ten-second intervals per prebuilt process; prepared delete/reinsert pairs rotate IDs. Count, exact ID/URI readback, shared-token exact IDs/multiplicity are validated after the timer. This is not a peer benchmark.

Broad variant:

| Pair | Baseline pairs/s | Candidate pairs/s | Gain |
|---|---:|---:|---:|
| 1 | 108947.9 | 141506.8 | 29.88% |
| 2 | 152279.5 | 169275.9 | 11.16% |
| 3 | 118795.3 | 155537.7 | 30.93% |

Removal-only variant:

| Pair | Baseline pairs/s | Candidate pairs/s | Gain |
|---|---:|---:|---:|
| 1 | 171165.2 | 171633.7 | 0.27% |
| 2 | 171942.7 | 161226.3 | -6.23% |
| 3 | 166580.5 | 123027.5 | -26.15% |

## Evidence and remaining work

- fixed-* is the normalized broad cohort; only-* is the normalized removal-only cohort.
- pair-* predates timestamp normalization and is exploratory, not a corrected peer proof.
- fixed-read confirms a normalized scalar read fixture passed its assertions.
- live-fixed-check verifies retained benchmark setup against current production code.
- All measured snapshot processes completed successfully, as did churn readback validators. Build-only Cargo filters intentionally selected no cases and returned 1 after compilation.
- Shared source contexts, variants, source archives, binaries/hashes, raw observations and background process inventory are retained.

The retained production change is benchmark correctness, not a library speedup. No eight-peer victory is claimed. Native deletion still requires improvement; exact-time native ingestion plus MSSQL/Kusto environments remain outstanding.

Live production verification: compare/delete_1row_100k/lin: 24.375 µs, compare/delete_1row_100k/sqlite: 19.249 µs (one process, 12 fresh fixtures; not an independent paired optimization proof). Fixture assertions and readback passed; scoped rustfmt/diff checks passed.


</details>

### 68. 2026-10-01-prepared-bulk-profile

Артефакты: [2026-10-01-prepared-bulk-profile](../benches/results/2026-10-01-prepared-bulk-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-prepared-bulk-profile/report.md).

# Prepared bulk sampling diagnostic

`cargo run --release --offline --example profile_bulk` repeatedly inserts 10000
prepared rows into a fresh memory Db with the scalar index. The loop includes
schema setup, affected-count/row-count assertions and destruction; it is not the
native benchmark timer. Preparation/source creation are before PROFILE_READY.
A five-second macOS sample (1 ms interval) captured 3786 main-thread stacks.
The complete 20.004-second run inserted 1192 batches and passed count checks.

Top-of-stack observations include memcmp 572, allocator free 401, tiny allocation
275, memmove 265, HashingEmbedder::fill 127, Scratch::bump 115, Scratch::finish 103,
Unicode character iterator 81, FtsIndex::append_row 79, lowercased 63 and Unicode
lower conversion 63. These are sample counts, not exact CPU/elapsed percentages.
FNV/content hashing can be inlined; absence of a named hot frame does not prove
its exact cost. Unicode conversion motivated the independently tested ASCII
path; this profile does not itself demonstrate a speedup or peer win.

Raw stacks are in sample.txt; the workload is examples/profile_bulk.rs.


</details>

### 69. 2026-10-01-prepared-cells-ab

Артефакты: [2026-10-01-prepared-cells-ab](../benches/results/2026-10-01-prepared-cells-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-prepared-cells-ab/report.md).

# Rejected prepared cell-value cache

Three independent process pairs, alternating order; 12 fresh fixtures per case.

| Pair | Original full 10k | Cached cells | Original no embed | Cached no embed |
|---|---:|---:|---:|---:|
| 1 | 12.310 ms | 15.948 ms | 8.485 ms | 9.718 ms |
| 2 | 12.499 ms | 12.399 ms | 8.277 ms | 9.207 ms |
| 3 | 12.305 ms | 11.595 ms | 8.098 ms | 7.614 ms |

The candidate was removed: small wins in two full-insertion pairs did not
establish consistent improvement; one pair regressed substantially. Additional
retained memory is not justified. Background load is uncontrolled. Workspace
tests and synthetic time/duplicate-field checks passed. Source variants, hashes
and raw runs are saved. No peer win is claimed.

These runs predate the subsequent large-insert WAL/affected-count correctness
fix. Actual in-memory rows were inserted, but affected-count and large WAL
behavior were not validated by these diagnostic phase benchmarks.


</details>

### 70. 2026-10-01-prepared-sorted-records

Артефакты: [2026-10-01-prepared-sorted-records](../benches/results/2026-10-01-prepared-sorted-records).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-prepared-sorted-records/report.md).

# Prepare-time sorted insertion records — rejected

Candidate stably sorts insert fields and collapses duplicates once after typecheck/plan during prepare (last value retained). Bulk execution collects the already sorted fields into BTreeMap. Relative timestamps still evaluated at execution; public source and plan retained from original program. Preparation work lies outside native insert timers under the existing prepared-query contract; this is a real compilation change, not disabled work.

Full offline workspace tests pass. Additional prepared insert test covers three duplicate title fields, deterministic evaluation at two supplied times, actual execution-time now and exact stored value. Test retained after reverting optimization. Current native profile in ../2026-10-01-native-profile-current/ motivated construction-path investigation; diagnostic is not peer timing evidence.

Six alternating process pairs, 24 fresh fixtures/case, one operation. Full native Lin, SQLite, DuckDB Appender; exact fields checked outside timer. Pinned optimized binaries; tests/builds did not overlap measurements. Host load uncontrolled, fixed case order. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 0.962812 | 0.977792 | 10.428895 | 10.344687 |
| 2 | 0.902437 | 0.929437 | 9.794854 | 10.362646 |
| 3 | 0.948896 | 0.945750 | 10.309126 | 9.994687 |
| 4 | 0.926729 | 0.918583 | 9.719188 | 9.942291 |
| 5 | 0.926000 | 0.895020 | 9.826291 | 9.796229 |
| 6 | 0.919312 | 0.900896 | 10.111688 | 9.852208 |

Both sizes win 4/6 pairs; median paired decreases 0.61% and 0.56%. SQLite controls median -0.91% and +0.97%, DuckDB +0.93% and +2.47%; no stable improvement beyond uncontrolled variation established. Candidate rejected; original prepare and row construction restored. Prior WAL/doc slab changes retained. All-eight-peer objective unproven. Raw observations and hashes retained.


</details>

### 71. 2026-10-01-prepared-write-profile

Артефакты: [2026-10-01-prepared-write-profile](../benches/results/2026-10-01-prepared-write-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-prepared-write-profile/report.md).

# Prepared write diagnostic profile

Command: `cargo run --release --offline --example profile_writes -- --rows 10000 --seconds 25`.
After PROFILE_READY, macOS `sample PID 5 1` recorded five seconds of stacks with a 1 ms interval.
The hot loop rotates through 10000 precompiled delete/insert pairs, keeping ID, URI,
title, body and wing; timestamps are evaluated at insertion. Seed/prepare and final
readback are outside the sampled loop. Deletes need CAS; the body hash stays fixed.

The run completed 3131541 pairs in 25.001 seconds and validated final row count and
every ID/URI lookup. This is a diagnostic churn workload, not a peer benchmark or
a measurement of fresh-fixture single-delete latency. Do not equate its pair rate
with native compare.rs delete throughput.

Among 4142 main-thread sampled stacks, top-of-stack counts include union_sorted 255,
subtract_sorted 143, FtsIndex::move_row 110 and FtsIndex::remove_row 85. These are
sampling observations, not exact elapsed-time percentages or allocation counts.
They justify investigating pending FTS folding and posting updates next. Repeated
churn amplifies delta folding; fresh one-row native mutations require their own A/B
measurements before retaining a change.

Full raw symbolized sample and source hashes are saved beside this report.


</details>

### 72. 2026-10-01-row-build-after

Артефакты: [2026-10-01-row-build-after](../benches/results/2026-10-01-row-build-after).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-row-build-after/report.md).

# Rejected bulk row construction

One independent process per variant, 12 fresh fixtures per case.

| 10k insertion | Original | Collect into BTreeMap |
|---|---:|---:|
| Full | 12.480 ms | 13.371 ms |
| No embedding | 8.909 ms | 9.394 ms |
| No embedding/scalar index | 8.007 ms | 8.355 ms |
| No embedding/scalar index/FTS | 5.691 ms | 6.441 ms |

All measured cases regressed. Background load was uncontrolled; this is not
universal performance proof. The experiment was removed. Full workspace tests
passed the candidate; duplicate-field and execution-time regression coverage
remains. No peer win is claimed.


</details>

### 73. 2026-10-01-row-build-before

Артефакты: [2026-10-01-row-build-before](../benches/results/2026-10-01-row-build-before).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/insert_phase_10k/lin_full`, `compare/insert_phase_10k/lin_no_embed`, `compare/insert_phase_10k/lin_no_embed_no_scalar_index`, `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts`.
- [progress.json](../benches/results/2026-10-01-row-build-before/progress.json)
- [run.json](../benches/results/2026-10-01-row-build-before/run.json)
- [status-final.json](../benches/results/2026-10-01-row-build-before/status-final.json)

### 74. 2026-10-01-segmented-embedding

Артефакты: [2026-10-01-segmented-embedding](../benches/results/2026-10-01-segmented-embedding).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-segmented-embedding/report.md).

# Segmented hashing embedding experiment — rejected

Candidate avoids allocating the joined title/body/snippet string for HashingEmbedder by processing borrowed segments. Preserves unigram/trigram then whole-string bigram order, including spaces crossing segment boundaries. Default Embedder method joins segments and delegates to existing embed_batch overrides. Candidate was tested in a git-archive copy of commit 8126c0c; conflicted main worktree was untouched. Patch is experimental and NOT integrated.

Bitexact tests pass: 512 field combinations at each of dimensions 8/768/4096, empty/missing fields, whitespace, NUL, Unicode expansions and long repeated inputs; exact joined text and f32 bits checked. Bench candidate compiled successfully.

Six alternating process pairs for native and durable inserts; 24 fresh fixtures per case, one operation. Defaults embedding, FTS and scalar index preserved. Exact inserted common fields checked outside timers. Durable inserts preserve WAL sync_data. No builds/tests overlapped timing; host load uncontrolled. Fixed case order; large outliers prevent causal attribution. Milliseconds are median of process medians, gains are median paired decreases.

| Path | Rows | Lin baseline | Candidate | Paired decrease | Wins | SQLite control decrease |
|---|---:|---:|---:|---:|---:|---:|
| native | 1k | 1.108021 | 1.290219 | -12.70% | 2/6 | -14.84% |
| native | 10k | 12.179334 | 13.471813 | 2.27% | 4/6 | -0.65% |
| durable | 1k | 2.452302 | 2.514563 | -2.30% | 1/6 | -0.12% |
| durable | 10k | 20.494104 | 19.831896 | 3.25% | 5/6 | -1.44% |

Reject: target native 1k loses 4/6 pairs with large outliers; unchanged SQLite control also regresses, so native effects cannot be isolated. Native 10k is inconclusive. Durable 10k improves 5/6 pairs (+3.25%), but durable 1k loses 5/6 (-2.30%) while its unchanged SQLite control is nearly flat (-0.12%). This does not solve the target small-insert deficit. No broad peer victory or production speedup claimed. Original snapshot sources restored after preserving the candidate patch. Main merge state left untouched.


</details>

### 75. 2026-10-01-serial-final-bulk

Артефакты: [2026-10-01-serial-final-bulk](../benches/results/2026-10-01-serial-final-bulk).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-serial-final-bulk/report.md).

# Final serial bulk verification

One process, 12 fresh-fixture samples per case. Setup is outside timing. Lin affected count and every row are checked outside timing; DuckDB Appender includes flush inside timing and checks stored fields outside timing.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.759 ms | 1.110 ms | 1.501 ms |
| 10k | 15.738 ms | 13.927 ms | 12.193 ms |

Lin still loses these ingestion cases. The slow DuckDB SQL row-loop case does not establish a win against its native Appender. Lin computes embeddings and maintains its native FTS/indexes; peer schemas/work differ as documented in the benchmark contract. Background load is uncontrolled; these 12 samples are not 12 independent processes.

Retained local optimizations have separate alternating-process evidence: sparse normalization (`../2026-10-01-norm-alternating/report.md`) and FTS append (`../2026-10-01-fts-append-ab/report.md`). Pool and scoped-thread experiments were reverted.

Large-insert WAL/readback safety and correct affected counts remain fixed. No universal peer win or goal completion is claimed.


</details>

### 76. 2026-10-01-shared-identity-ab

Артефакты: [2026-10-01-shared-identity-ab](../benches/results/2026-10-01-shared-identity-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-shared-identity-ab/report.md).

# Shared identity keys: retained for ingestion, mutation performance unresolved

Private by-ID and document-URI maps now hold Arc<str> keys sharing immutable cell text instead of allocating String copies. Public Row and persisted row/column formats are unchanged. Rebuild, single registration and slab registration use shared keys.

Initial live-workspace builds were rejected because src/exec.rs changed concurrently. Actual compared binaries were built from a frozen source snapshot with only src/store.rs differing. Snapshot build logs, common hashes and source-snapshot.tar.gz preserve this context. Default features only; no GPU benchmark claim is made.

## Six paired bulk and point-read processes

16 fresh fixtures per insertion phase; 24 calibrated warm point-read observations per process. Builds completed before measurement. Alternating process order. All phases still register identity maps; there is no unchanged identity-map control.

| Pair | Baseline full ms | Shared full ms | Paired reduction | Baseline point ns | Shared point ns |
|---|---:|---:|---:|---:|---:|
| 1 | 11.027 | 11.275 | -2.3% | 253.57 | 242.83 |
| 2 | 12.155 | 11.312 | 6.9% | 234.66 | 244.50 |
| 3 | 12.242 | 11.139 | 9.0% | 232.18 | 240.76 |
| 4 | 13.483 | 16.177 | -20.0% | 285.41 | 297.17 |
| 5 | 13.845 | 11.975 | 13.5% | 263.77 | 253.35 |
| 6 | 14.163 | 12.307 | 13.1% | 303.96 | 289.84 |

Median paired full-insert reduction 8.0%, 4/6 wins. Point-read median paired change 0.13%, effectively tied with 3/6 wins. Load is uncontrolled and timings fluctuate. No statistical significance or universal causal percentage is claimed.

## Matched single-row writes

Three independent process pairs; 12 fresh fixtures per case. Validators outside timing. These expose an unresolved tradeoff rather than a confirmed single-row improvement.

| Pair | Case | Baseline Lin µs | Shared Lin µs | Baseline SQLite µs | Shared-process SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | update 1k | 3.999 | 5.625 | 4.667 | 4.646 |
| 1 | update 10k | 8.688 | 8.667 | 10.979 | 8.416 |
| 1 | update 100k | 14.501 | 13.438 | 15.958 | 15.562 |
| 1 | delete 1k | 6.333 | 9.521 | 3.062 | 4.021 |
| 1 | delete 10k | 13.500 | 13.521 | 9.834 | 7.458 |
| 1 | delete 100k | 21.541 | 23.062 | 13.062 | 18.666 |
| 2 | update 1k | 8.312 | 6.104 | 4.312 | 5.354 |
| 2 | update 10k | 9.354 | 8.334 | 14.000 | 11.625 |
| 2 | update 100k | 13.833 | 15.771 | 19.854 | 14.896 |
| 2 | delete 1k | 11.583 | 9.834 | 6.333 | 4.167 |
| 2 | delete 10k | 17.375 | 14.666 | 15.583 | 9.979 |
| 2 | delete 100k | 21.895 | 22.812 | 14.166 | 19.084 |
| 3 | update 1k | 5.062 | 6.041 | 3.146 | 4.854 |
| 3 | update 10k | 8.209 | 8.896 | 9.166 | 9.374 |
| 3 | update 100k | 13.688 | 15.937 | 18.729 | 19.500 |
| 3 | delete 1k | 6.021 | 5.146 | 3.979 | 4.750 |
| 3 | delete 10k | 14.479 | 14.250 | 7.938 | 8.999 |
| 3 | delete 100k | 23.750 | 26.813 | 17.854 | 19.208 |

100k update lost two of three baseline comparisons. Shared-process Lin beat SQLite in two of three 100k update runs, but a stable peer win is not proved. Single-row performance requires further work; it is not being marked complete.

## URI-only alternative

Keeping String IDs while sharing only URI keys won only 1/3 full insertion pairs; it was not selected. Corresponding write timings are also included in the uri-write raw runs.

| Pair | Baseline full ms | URI-only full ms | Reduction |
|---|---:|---:|---:|
| 1 | 12.222 | 12.433 | -1.7% |
| 2 | 18.713 | 13.818 | 26.2% |
| 3 | 12.031 | 12.622 | -4.9% |

## Native bulk snapshot verification

One process, 12 fresh fixtures, count/row readback outside timing; Appender flush inside timing. Setup/schema differences are documented in the contract. These timings prove no complete peer victory.

| Rows | Shared Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.039 | 0.810 | 1.179 |
| 10k | 11.002 | 10.286 | 10.219 |

Lin still loses 10k ingestion; native deletion also remains slower than SQLite. The first native-delete run overlapped workspace compilation/tests and is explicitly excluded. native-delete-clean ran after those checks ended, with passing validators; its raw timings remain separate from matched write comparisons.

## Validation and decision

Complete default workspace tests passed on the current working tree after the shared-key change, including rollback, cold readback, checkpoint/reopen, FTS and WAL. The shared-key implementation is retained as a partial ingestion/memory improvement while the single-row tradeoff remains unresolved. This does not close the objective or justify dropping update/delete requirements. Existing confirmed FTS/ASCII/normalization changes remain. Scoped rustfmt and git diff --check pass.


</details>

### 77. 2026-10-01-shared-wal-vectors

Артефакты: [2026-10-01-shared-wal-vectors](../benches/results/2026-10-01-shared-wal-vectors).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-shared-wal-vectors/report.md).

# Shared WAL vectors experiment — rejected

Internal ColData vector payload changed from owned Vec to Arc slices; writer packing and row reconstruction share Cell storage. Wire format unchanged, tested against legacy MessagePack bytes. Full workspace tests and a sharing/serialization test pass. The existing conservative decoded-vector budget remains unchanged.

Six alternating process pairs; first three use 16 fresh fixtures, next three 24. One operation per fixture; exact row checks outside timing; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap measurements; host load uncontrolled. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 3.065437 | 3.283917 | 27.054729 | 30.167333 |
| 2 | 3.799479 | 3.370167 | 34.973583 | 27.724021 |
| 3 | 3.601854 | 3.371125 | 30.802000 | 31.144270 |
| 4 | 3.586291 | 4.393750 | 29.418833 | 36.298541 |
| 5 | 3.100750 | 3.226646 | 26.834521 | 27.977521 |
| 6 | 3.670063 | 3.959833 | 33.243167 | 31.644646 |

Candidate wins 2/6 pairs at both sizes. Median paired duration changes: 1k +5.59%, 10k +2.69% (slower). SQLite controls varied substantially; no isolated causal slowdown asserted. Eliminated payload copies are verified, but sustained benchmark improvement is not. Candidate rejected and previous production source restored. Raw observations, candidate source and medians retained. Overall all-eight-peer goal remains unproven.


</details>

### 78. 2026-10-01-sparse-single-pass

Артефакты: [2026-10-01-sparse-single-pass](../benches/results/2026-10-01-sparse-single-pass).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-sparse-single-pass/report.md).

# Sparse WAL single-pass experiment — rejected

Compared pinned optimized baseline/candidate binaries in three alternating process pairs, 16 fresh fixtures per case, one operation per fixture. Exact rows validated outside timing; durable sync unchanged. No build/test process overlapped these measurements. Host load uncontrolled.

Candidate replaced the separate nonzero-count scan with an increment during serialization and backfilled the count. Wire representation unchanged.

| Pair | Lin 1k baseline ms | candidate ms | Lin 10k baseline ms | candidate ms |
|---:|---:|---:|---:|---:|
| 1 | 2.9367 | 2.6244 | 25.6875 | 25.1630 |
| 2 | 3.1445 | 2.9491 | 25.1734 | 26.6844 |
| 3 | 2.7148 | 2.6631 | 26.3431 | 26.6548 |

1k improved 3/3 pairs (median paired decrease 6.22%), while 10k improved only 1/3 (median paired decrease -1.18%). Unchanged SQLite controls varied substantially. No consistent benefit established for the larger workload; candidate reverted. Previous sparse codec and decoder bounds remain. Full raw observations and medians.json are alongside this report. Overall all-peer objective remains unproven.


</details>

### 79. 2026-10-01-sparse-vector-wal

Артефакты: [2026-10-01-sparse-vector-wal](../benches/results/2026-10-01-sparse-vector-wal).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-sparse-vector-wal/report.md).

# Sparse-vector WAL: 2026-10-01

10,000 default embedded documents now persist as one checksummed atomic WAL frame: **4,624,412 bytes**. Dense vector storage alone would require 30,760,000 bytes, excluding other fields. The frame ceiling remains 16 MiB.

## Durable insert measurements

Milliseconds; median of eight fresh fixtures per process. Three independent processes, same case order; host load uncontrolled. Setup, teardown and exact field validation are outside the timer. Lin Full fsync and SQLite synchronous FULL remain enabled. Lin also generates embeddings, FTS and CAS. These are workload comparisons, not proof of isolated codec speedup.

| Rows | Process | Lin ms | SQLite ms |
|---:|---:|---:|---:|
| 1k | 1 | 3.065062 | 1.413833 |
| 1k | 2 | 2.971750 | 1.649875 |
| 1k | 3 | 2.900729 | 1.403979 |
| 1k | median of process medians | 2.971750 | 1.413833 |
| 10k | 1 | 25.149438 | 13.684708 |
| 10k | 2 | 26.670146 | 13.870417 |
| 10k | 3 | 25.940749 | 13.750146 |
| 10k | median of process medians | 25.940749 | 13.750146 |

Lin remains slower than SQLite in these durable insert measurements. The former 10k run failed the WAL size limit; it cannot serve as a timing baseline. No overall victory over all eight requested peers is established.

## Verification and compatibility

Full workspace offline tests pass, including exact local WAL replay, replica apply, checkpoint/reopen, float bit preservation, corruption and malformed sparse payload bounds. Source checks and hashes are saved alongside this report.

New writers may emit codec 3 inside checksummed LIN06. Every reader/follower must be upgraded before consuming it. Small/dense vectors retain codec 2; edge-bearing inserts can use the existing MessagePack fallback. Decoded sparse vectors are limited to 64 MiB per record. This does not remove all large-batch limits.


</details>

### 80. 2026-10-01-stream-delete-ab

Артефакты: [2026-10-01-stream-delete-ab](../benches/results/2026-10-01-stream-delete-ab).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-stream-delete-ab/report.md).

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


</details>

### 81. 2026-10-01-substr-packed

Артефакты: [2026-10-01-substr-packed](../benches/results/2026-10-01-substr-packed).

<details>
<summary>process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-packed/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 234.044 | 1.00× | validated |
| text_substr | duckdb | 485.484 | 2.07× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-packed/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | duckdb | 480.533 | 2.30× | validated |
| text_substr | lin | 209.177 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-packed/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 207.297 | 1.00× | validated |
| text_substr | duckdb | 485.394 | 2.34× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-packed/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 209.177 | 1.00× | validated |
| text_substr | duckdb | 485.394 | 2.32× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

### 82. 2026-10-01-substr-short-scan

Артефакты: [2026-10-01-substr-short-scan](../benches/results/2026-10-01-substr-short-scan).

<details>
<summary>process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-short-scan/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 723.844 | 1.00× | validated |
| text_substr | duckdb | 569.174 | 0.79× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-short-scan/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | duckdb | 658.899 | 0.91× | validated |
| text_substr | lin | 724.229 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-short-scan/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 770.292 | 1.00× | validated |
| text_substr | duckdb | 640.072 | 0.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-substr-short-scan/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 724.229 | 1.00× | validated |
| text_substr | duckdb | 640.072 | 0.88× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.


</details>

### 83. 2026-10-01-wal-cell-refs

Артефакты: [2026-10-01-wal-cell-refs](../benches/results/2026-10-01-wal-cell-refs).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-wal-cell-refs/report.md).

# Uniform WAL cell-reference cache

Uniform rows now traverse each BTreeMap once to collect cell references. Column inference and construction then access references by position, avoiding repeated tree lookups. Heterogeneous rows keep field-name lookups. Values and wire format unchanged; references live only during pack construction. Temporary memory is row count × field count × pointer size, plus Vec capacity overhead; no cloned cells in this cache.

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Full exact-row validation outside timer; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap timings. Host load uncontrolled; fixed case order within each process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.887958 | 3.014417 | 26.585959 | 24.342042 |
| 2 | 3.098854 | 2.997395 | 25.060980 | 23.838500 |
| 3 | 3.074667 | 2.745479 | 25.230354 | 23.845000 |
| 4 | 2.889959 | 2.824542 | 24.700958 | 24.001271 |
| 5 | 3.330626 | 2.509792 | 23.607687 | 23.073562 |
| 6 | 2.779813 | 2.673875 | 27.088042 | 23.482687 |

Candidate faster in 5/6 pairs at 1k, median paired decrease 3.54%; at 10k faster in 6/6, median decrease 5.18%. Unchanged SQLite controls have median paired decreases 1.79% and -0.04%, respectively. No significance or isolated causal percentage claim. Change retained.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.785011 | 1.472865 |
| 10k | 23.841750 | 14.053677 |

Full offline workspace tests pass, including heterogeneous field union and exact sparse-vector local WAL replay/replica apply/checkpoint/reopen. Scoped formatting and diff checks pass. No GPU measurements. Lin still loses durable insert to SQLite; full eight-peer objective unproven. Raw observations, source baseline and hashes retained.


</details>

### 84. 2026-10-01-wal-shared-text

Артефакты: [2026-10-01-wal-shared-text](../benches/results/2026-10-01-wal-shared-text).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-wal-shared-text/report.md).

# Shared text in WAL column packs

Internal ColData::Text uses Arc<str>, sharing existing Cell text when constructing a WAL pack and reconstructing rows. The temporary pack no longer allocates a String for every text cell. Wire encodings and default values unchanged; missing text still becomes an empty string. Public API unchanged (persist module private).

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Full exact-row validation outside timer; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap timings. Host load uncontrolled and fixed case order within a process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.806792 | 2.767709 | 23.900250 | 22.800896 |
| 2 | 2.925458 | 2.602208 | 23.340312 | 22.575875 |
| 3 | 3.000584 | 2.597167 | 23.293521 | 21.708230 |
| 4 | 4.148854 | 2.939833 | 23.831312 | 22.403604 |
| 5 | 3.309125 | 2.729376 | 23.660604 | 22.970917 |
| 6 | 2.756417 | 2.710876 | 23.421208 | 22.635000 |

Candidate faster in 6/6 pairs at both sizes. Median paired decrease 12.25% at 1k and 3.98% at 10k. Unchanged SQLite controls also improved by median 5.96% and 2.13%, with a large outlier. Observed percentages are not isolated causal attribution or statistical significance claims. Retain the change.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.720125 | 1.425000 |
| 10k | 22.605438 | 13.954667 |

Full offline workspace tests pass. Added compatibility test verifies byte-identical MessagePack versus legacy Vec<String>, legacy decoding, empty/Unicode/NUL text, exact reconstructed rows and shared Arc identity. Scoped rustfmt and git diff checks pass. Lin still slower than SQLite in these durable inserts; no all-eight-peer victory established. Raw observations, baseline source and hashes retained.


</details>

### 85. 2026-10-01-wal-uniform-fields

Артефакты: [2026-10-01-wal-uniform-fields](../benches/results/2026-10-01-wal-uniform-fields).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-wal-uniform-fields/report.md).

# WAL uniform field-set fast path

Skip redundant BTreeSet insertions when a row has exactly the same ordered field names as the first row. Heterogeneous rows retain the full union path. Field names are still copied once into the owned pack; column inference, encoding, atomicity and fsync unchanged.

Six alternating process pairs, 24 fresh fixtures per case, one operation per fixture. Full exact-row validation outside timer. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled and case order fixed within a process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.825875 | 2.918687 | 24.548562 | 22.467834 |
| 2 | 2.970584 | 2.741208 | 23.399541 | 22.424687 |
| 3 | 2.947125 | 2.953187 | 23.685541 | 22.495833 |
| 4 | 2.958958 | 2.850708 | 24.200812 | 22.830354 |
| 5 | 3.027958 | 3.657458 | 24.290354 | 23.094896 |
| 6 | 2.863791 | 2.730396 | 23.711688 | 22.354000 |

10k candidate faster in 6/6 pairs; median paired decrease 5.34%. Unchanged SQLite controls also faster in 6/6, median decrease 1.99%; percentages are observed changes, not an isolated causal claim. 1k is mixed: 3/6 wins and median paired decrease 1.73%, with one marked regression. Retain for repeatable large-batch improvement, without claiming a small-batch speedup.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.884698 | 1.355688 |
| 10k | 22.481834 | 13.280458 |

Full offline workspace tests pass. Additional union test covers homogeneous rows, equal-sized rows with different keys, and later extra fields, verifying reconstructed values including existing missing-integer defaults. Scoped formatting and diff checks pass. Lin still loses these durable insert comparisons to SQLite; all-eight-peer objective remains unproven. Raw observations, baseline source and binary/source hashes retained.


</details>

### 86. 2026-10-01-wal-zstd

Артефакты: [2026-10-01-wal-zstd](../benches/results/2026-10-01-wal-zstd).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-01-wal-zstd/report.md).

# Zstd sparse WAL compression experiment — rejected

Candidate introduces checksummed internal codec 4 containing original sparse codec, bounded decoded length and Zstd level-1 data. Only sparse payloads >=256KiB are candidates; use compressed frame only when smaller. Original 16MiB decoded frame budget and 64MiB sparse-vector budget preserved. Full fsync unchanged. Old codecs readable, but older readers would not understand new codec 4. This candidate was rejected and is not shipped.

10k default embedded document frame shrinks from 4,624,412 B to 427,472 B (10.82x). Exact local WAL replay, replica apply and checkpoint/reopen test passes. Full workspace offline tests pass before final decoder restriction; final targeted tests verify only sparse codec allowed, declared decoded lengths, truncation and outer CRC, plus exact 10k replay under final decoder.

Six alternating process pairs, 24 fresh fixtures/case, one operation each. Exact field validation outside timer. Pinned optimized binaries; builds/tests did not overlap measurements. Host load uncontrolled; fixed case order per process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.454730 | 2.896187 | 19.591916 | 23.767834 |
| 2 | 2.634812 | 2.839354 | 21.952708 | 23.864604 |
| 3 | 2.444438 | 2.960625 | 20.310520 | 22.962229 |
| 4 | 2.442562 | 2.957167 | 20.515563 | 23.332625 |
| 5 | 2.741313 | 2.973687 | 21.039271 | 23.108708 |
| 6 | 2.496688 | 2.885187 | 20.392916 | 23.480021 |

Candidate loses 6/6 pairs at both sizes. Median paired decreases -16.77% (1k), -13.39% (10k), meaning slower. SQLite controls median +0.35% and -1.06%. Size savings did not translate to speed on this host/workload; no isolated kernel or causality claim. Candidate codec/dependency/integration-test change reverted. Prior sparse codec 3 and WAL/document-map improvements retained. All-eight-peer speed goal remains unproven. Raw observations, candidate sources, test logs and hashes retained.


</details>

### 87. 2026-10-01-write-after

Артефакты: [2026-10-01-write-after](../benches/results/2026-10-01-write-after).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-write-after/progress.json)
- [run.json](../benches/results/2026-10-01-write-after/run.json)
- [status-final.json](../benches/results/2026-10-01-write-after/status-final.json)

### 88. 2026-10-01-write-before-verified

Артефакты: [2026-10-01-write-before-verified](../benches/results/2026-10-01-write-before-verified).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-write-before-verified/progress.json)
- [run.json](../benches/results/2026-10-01-write-before-verified/run.json)
- [status-final.json](../benches/results/2026-10-01-write-before-verified/status-final.json)

### 89. 2026-10-01-write-borrowed-tokens

Артефакты: [2026-10-01-write-borrowed-tokens](../benches/results/2026-10-01-write-borrowed-tokens).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-write-borrowed-tokens/progress.json)
- [run.json](../benches/results/2026-10-01-write-borrowed-tokens/run.json)
- [status-final.json](../benches/results/2026-10-01-write-borrowed-tokens/status-final.json)

### 90. 2026-10-01-write-final

Артефакты: [2026-10-01-write-final](../benches/results/2026-10-01-write-final).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-write-final/progress.json)
- [run.json](../benches/results/2026-10-01-write-final/run.json)
- [status-final.json](../benches/results/2026-10-01-write-final/status-final.json)

### 91. 2026-10-01-write-owned-move

Артефакты: [2026-10-01-write-owned-move](../benches/results/2026-10-01-write-owned-move).

Отдельного Markdown-описания нет; доступны сырые прогоны/профили. Решение о сохранении оптимизации по одному имени папки не выводится.

Измеренные cases: `compare/delete_1row_100k/lin`, `compare/delete_1row_100k/sqlite`, `compare/delete_1row_10k/lin`, `compare/delete_1row_10k/sqlite`, `compare/delete_1row_1k/lin`, `compare/delete_1row_1k/sqlite`, `compare/update_1row_100k/lin`, `compare/update_1row_100k/sqlite`, `compare/update_1row_10k/lin`, `compare/update_1row_10k/sqlite`, `compare/update_1row_1k/lin`, `compare/update_1row_1k/sqlite`.
- [progress.json](../benches/results/2026-10-01-write-owned-move/progress.json)
- [run.json](../benches/results/2026-10-01-write-owned-move/run.json)
- [status-final.json](../benches/results/2026-10-01-write-owned-move/status-final.json)

### 92. 2026-10-02-merged-validation

Артефакты: [2026-10-02-merged-validation](../benches/results/2026-10-02-merged-validation).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-02-merged-validation/report.md).

# Applied merge validation

User authorized resolving the pending merge on 2026-10-02. Applied the tested resolution: preserve current embedding/FTS/WAL/slab implementations, retain Embedder.embed_batch_owned default API, hybrid top-k selection and large-bulk reopen regression. Source formatting completed.

Final main-tree checks: cargo test --offline --workspace PASS; cargo bench --offline --bench compare --no-run PASS; git diff --cached --check for src/tests PASS. Test outputs preserved verbatim. Hybrid bounded/full-rank test and large-bulk reopen regression both pass. Earlier isolated performance evidence is in ../2026-10-01-merge-proposal/report.md; no new all-peer or GPU performance claim. MSSQL/Kusto comparison remains missing.


</details>

### 93. 2026-10-02-mysql-owned-native

Артефакты: [2026-10-02-mysql-owned-native](../benches/results/2026-10-02-mysql-owned-native).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-02-mysql-owned-native/report.md).

# MySQL owned-table and native multi-row INSERT comparison

Final source forces ENGINE=InnoDB for all MySQL fixture tables. Each fixture has its own generated table prefix; static application docs/docs_bulk/users/orders/logs_bulk tables are not reused. Fixture Drop cleans only its owned names. Regular tables, indexes and transactions are used, not temporary or unlogged tables. CREATE/DROP TABLE privileges on the selected database suffice; no CREATE DATABASE privilege required by the benchmark.

Native mysql_batch: one server-prepared INSERT with all rows (6k or 60k parameters), preparation and schema outside timing; parameter encoding/allocation, transaction, execute and commit inside timing. Both rowwise and native insert cases read all six common fields from a separate connection outside timing and compare exact sorted tuples, including multiplicity. Matching common schema, indexes and identity constraints; Lin retains default hashing embedding, FTS and scalar index maintenance.

Dedicated Docker MySQL 8.4.11 with data on tmpfs, innodb_flush_log_at_trx_commit=1, sync_binlog=1, max_allowed_packet=64MiB. This includes client/server round-trip and transfer; it does not prove physical-disk durability or intrinsic engine CPU performance. COPY, LOAD DATA and alternative drivers are not measured here; no claim of best possible MySQL ingestion.

Three independent processes, eight fresh fixtures per case, one timed operation, no warmup. Schema, preparation, readback validation and input destruction excluded; output drop included. Fixed case order, uncontrolled host load. Medians within each process, then median of process medians. No compilation/tests overlapped timing. Initial run without explicit ENGINE is retained separately; final/ is authoritative.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms | MySQL batch ms |
|---|---:|---:|---:|---:|
| 1k | 0.913333 | 0.787813 | 1.162604 | 5.987125 |
| 10k | 9.639209 | 10.168708 | 10.006250 | 68.459625 |

Safety: 11 selected */mysql cases complete in the dedicated validation database. Five sentinel tables and their exact rows remain unchanged after each of four final processes. Zero fixture tables remain. Native and rowwise insert readbacks pass. Validation database/grant removed; owned MySQL container stopped on completion. Verification, raw results and binary hashes in final/. Cargo bench compilation, scoped rustfmt and source diff checks pass. Library code unchanged; full library tests not rerun for this harness-only change.

The eight-peer target remains unproven: small inserts still lose to SQLite; DuckDB 10k comparison is not a stable Lin win; MSSQL/Kusto are unavailable; Mongo native ingestion is not covered. Missing peers are never counted as wins.


</details>

### 94. 2026-10-03-embed-bigram-table

Артефакты: [2026-10-03-embed-bigram-table](../benches/results/2026-10-03-embed-bigram-table).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-embed-bigram-table/report.md).

# Default embedding bigram lookup: retained

Baseline: 73e2f18. Replace repeated FxHasher slice hashing and modulo for whole-string byte bigrams with a shared 65,536-entry u16 table for dimension 768. Other dimensions retain the original path. The table is computed using the active platform's FxHasher, not baked hash constants. Feature order, weights, normalization, model identity and persisted vector bits remain unchanged.

Memory cost: 128 KiB of process-lifetime table payload plus allocation/OnceLock metadata. Construction initializes the table once, before embedding. Twelve independent release processes measured the actual first HashingEmbedder constructor: median 72,583.5 ns (about 73 us), including ordinary construction. Subsequent constructors were 17–24 ns in the first observed runs (see all raw values). No baseline cold-start comparison was measured; this is an explicit new one-time cost, not a cold-start improvement. Diagnostic constructor source and binary hash are saved; the temporary example was removed.

Six alternating baseline/candidate process pairs for each workload. Native and durable insert cases: 24 fresh fixtures per process, one operation/sample, no warmup. Setup includes constructing the database and therefore excludes the new initialization cost from measured insert time, as well as existing preparation/schema costs. Fixture validation checks affected rows and values outside timing. Compilation and tests did not overlap timed runs. SQLite controls are included below.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 4.87% | 6/6 | 0.977062 | 0.911980 |
| compare/insert_bulk_1k/sqlite | 3.12% | 4/6 | 0.835094 | 0.809031 |
| compare/insert_bulk_10k/lin | 2.48% | 5/6 | 10.388791 | 10.169438 |
| compare/insert_bulk_10k/sqlite | -0.36% | 2/6 | 10.708719 | 10.727854 |
| compare/durable_insert_1k/lin | 0.27% | 4/6 | 2.611375 | 2.592188 |
| compare/durable_insert_1k/sqlite | 5.75% | 5/6 | 1.604521 | 1.539021 |
| compare/durable_insert_10k/lin | 3.28% | 6/6 | 20.463980 | 19.804865 |
| compare/durable_insert_10k/sqlite | 0.98% | 4/6 | 14.114812 | 14.017531 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i), not a ratio of separate aggregate medians. Native 1k gains are partly confounded by SQLite control movement; durable 1k shows little improvement. Native 10k gains and durable 10k gains are more consistent than their controls, with durable 10k faster in all six pairs. This does not establish a universal improvement across machines/text distributions.

Warm hybrid-common read: six alternating pairs, 32 samples, 50 ms warmup, target 5 ms/sample, at most 100 iterations. Median paired improvement 0.09%, faster 4/6: effectively unchanged in this evidence.

Validation: all 65,536 byte pairs match the original slice hash/modulo; reference embedding tests match exact f32 bits across dimensions 8/9/32/384/768/1024/1536 and ASCII/Unicode/empty/repeated texts. All six focused embedding tests passed. Full `cargo test --offline --workspace` passed. Benchmark builds and diagnostic constructor release build passed. `git diff --check` passed. No wire-format or public signature change.

Decision: retain for the measured large-insert gains with the explicit memory and cold-start tradeoff. SQLite still wins native 1k and durable 1k/10k; Lin wins native 10k in these process medians. MSSQL and Kusto remain unmeasured without endpoints; the full nine-engine goal is not achieved.


</details>

### 95. 2026-10-03-embed-zero-marker

Артефакты: [2026-10-03-embed-zero-marker](../benches/results/2026-10-03-embed-zero-marker).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-embed-zero-marker/report.md).

# Rejected embedding zero-marker experiment

Removed Scratch.seen, using a zero accumulator slot as the untouched marker.
All feature weights are positive and finish resets touched slots. Existing bit
reference, batch reuse and normalization tests passed (5 tests). Six alternating
independent process pairs per mode, 24 fresh one-operation samples per case,
no warmup. Fixture setup/drop excluded; no builds or tests overlapped timing.

| Lin case | Baseline ms | Candidate ms | Paired median gain | Wins |
|---|---:|---:|---:|---:|
| Native 1k | 1.0222 | 0.9849 | 3.83% | 6/6 |
| Native 10k | 11.0968 | 10.6345 | 4.70% | 5/6 |
| Durable 1k | 2.6532 | 2.7568 | -5.29% | 1/6 |
| Durable 10k | 21.0068 | 20.5908 | 2.00% | 6/6 |

SQLite durable 1k control had paired median gain 0.06% and wins 3/6.
Rejected because the small durable workload regressed while its SQLite control
was stable. The mechanism of that regression is not established by these
wall-clock measurements. Raw samples, binary/context/source hashes and candidate
source are retained. Production embed.rs is restored exactly to baseline.

The full eight-engine objective is unproven. Small native and durable insertion
still loses to SQLite in these runs. SQL Server and Kusto test endpoints are
absent. Next investigation should profile small durable insertion/WAL costs,
rather than infer durability speed from native embedding improvements.

Full candidate cargo test --offline --workspace passed. Baseline source was
restored byte-for-byte; baseline workspace validation is recorded in the
previous FTS optimization evidence. Trailing blank log lines were normalized;
raw JSON observations are unchanged.


</details>

### 96. 2026-10-03-fts-inline-postings

Артефакты: [2026-10-03-fts-inline-postings](../benches/results/2026-10-03-fts-inline-postings).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-fts-inline-postings/report.md).

# Inline FTS postings: rejected

Baseline: 7eaffc5. Candidate stores the first posting inline using SmallVec<[usize; 1]>; wire format and pending additions/deletions remain unchanged. Both sources and binary hashes are saved here.

Six independent alternating baseline/candidate pairs per workload, without overlapping compilation or tests. Insert workloads use 24 fresh fixtures per process, one operation per sample and no warmup; lexical reads use 32 samples, 50 ms warmup and up to 100 iterations. Exact fixture/read checks in the benchmark passed. Focused library FTS tests passed (12 tests); both benchmark binaries built successfully. Full workspace tests were not run for this rejected candidate.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| Native insert 1k | 2.34% | 5/6 |
| Native insert 10k | 1.38% | 5/6 |
| Durable insert 1k | -0.39% | 3/6 |
| Durable insert 10k | 1.41% | 5/6 |
| Lexical common | 4.84% | 4/6 |
| Lexical selective | 0.86% | 3/6 |
| Lexical miss | -3.09% | 2/6 |

Speedup is median(100 * (baseline_i - candidate_i) / baseline_i), not the ratio of separate aggregate medians. SQLite controls changed -0.28%, +1.11%, -1.27%, +0.21% respectively for the four insert workloads. The native 10k improvement is close to the control movement. Read timings show substantial process variability; the miss result does not establish a causal regression from inline storage.

Decision: reject this additional representation complexity given the small write improvements and inconclusive read results. src/fts.rs restored byte-for-byte to baseline. Raw run.json files, logs and summary medians remain for review. SQLite still wins native 1k and both durable sizes; Lin wins native 10k. This experiment does not complete the nine-engine performance goal.


</details>

### 97. 2026-10-03-fts-inline-tokens

Артефакты: [2026-10-03-fts-inline-tokens](../benches/results/2026-10-03-fts-inline-tokens).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-fts-inline-tokens/report.md).

# Rejected inline FTS token storage

Six alternating process pairs, 24 fresh one-operation samples per case.
1k delete: paired median gain -0.74%, wins 3/6.
10k delete: paired median gain 7.69%, wins 5/6.
Rejected because no consistent improvement across sizes. FTS unit tests passed
11 cases via pinned direct binary; cargo launch was terminated while stalled
in dyld startup. No production change retained. Raw samples and binary hashes
are retained.


</details>

### 98. 2026-10-03-fts-sorted-union

Артефакты: [2026-10-03-fts-sorted-union](../benches/results/2026-10-03-fts-sorted-union).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-fts-sorted-union/report.md).

# FTS sorted posting union

Retained optimization: candidate_idxs consumes already sorted unique live
posting lists directly. The first matching list moves into the result; later
lists merge linearly instead of inserting every row into a BTreeSet. Pending
additions/deletions still pass through the existing matches merge. Returned
positions remain sorted and unique.

Six alternating independent process pairs; 32 samples per case, warmup 50ms,
sample target 5ms, at most 100 operations per sample. Fixture setup excluded.
Same benchmark binary sources except src/fts.rs; no builds/tests overlap timing.
Host load is uncontrolled. Process medians aggregated before comparison.

| Query on 10k docs | Baseline µs | Candidate µs | Paired median gain | Winning pairs |
|---|---:|---:|---:|---:|
| Frequent term | 184.383 | 162.038 | 12.14% | 6/6 |
| Selective term | 0.731 | 0.696 | 6.03% | 5/6 |
| Missing term | 0.276 | 0.265 | 4.82% | 5/6 |

The strongest evidence is the frequent-term case. Submicrosecond differences
are sensitive to noise. This is an internal before/after comparison; peer
engine speed and durable insertion are not inferred from it. Raw samples,
source copies and SHA256 hashes are retained. A scan-reference regression test
covers multiple/duplicate/Unicode/missing terms with pending edits.

Validation: cargo test --offline --workspace passed, including the new scan-reference
regression. Final cargo bench --offline --bench compare --no-run passed. Scoped
rustfmt, Python benchmark contract tests (7), and git diff --check passed.


</details>

### 99. 2026-10-03-insert-allocation-profile

Артефакты: [2026-10-03-insert-allocation-profile](../benches/results/2026-10-03-insert-allocation-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-insert-allocation-profile/report.md).

# Diagnostic insert allocation counts

Dedicated single-thread Rust example with a temporary counting global allocator.
Three processes, three fresh fixtures per size per process; all nine counts
match exactly. Prepared source, schema and validation are outside counter scopes.
Same six common document fields plus wiki layer and default embedding/FTS;
constant timestamp and two wing/ts keys. Exact six-field readback and 768-dimension
embedding presence checked after each insert. Counters cover alloc, alloc_zeroed
and realloc calls; bytes are requested allocation/reallocation sizes, not live
or peak memory. Deallocation is not counted. The diagnostic allocator changes
runtime overhead/optimization, so no speed comparison is made from these runs.

| Stage | 1k calls | 1k requested bytes | 10k calls | 10k requested bytes |
|---|---:|---:|---:|---:|
| Row build/checks | 16,003 | 866,648 | 160,003 | 8,606,272 |
| Embedding | 3,010 | 3,188,160 | 30,011 | 31,865,116 |
| Scalar index | 31 | 414,617 | 42 | 3,441,585 |
| FTS | 2,060 | 264,881 | 20,081 | 2,543,785 |
| Row-map registration | 2 | 8 | 2 | 8 |

Row build includes record construction/default hash/uniqueness/FK checks and
initial built/check-set buffers. Row-map/index reserve calls between measured
stages, edges, row movement, final pack/Handle construction and subsequent drops
are excluded. Counts are stage-specific and not the entire insert allocation
budget. Record keys require owned Strings under the stable public Row alias;
text Cell construction creates Arc allocations for prepared String literals.
A distinct next candidate is shared AST text literals so value_cell can clone
existing Arc storage without a separate side cache, preserving stable Row/Cell
behavior. Earlier side-cache and sorted-row experiments were rejected and must
not be treated as performance proof for this candidate.

Build and all three diagnostic processes passed. Production exec.rs/lib.rs and
Cargo.toml restored byte-for-byte before running the pinned executable. Diagnostic
sources, hashes, raw logs and allocation ranges retained. No production allocator
or hooks retained. Full eight-engine objective is unproven.


</details>

### 100. 2026-10-03-insert-borrowed-text

Артефакты: [2026-10-03-insert-borrowed-text](../benches/results/2026-10-03-insert-borrowed-text).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-insert-borrowed-text/report.md).

# Borrow row text during bulk insertion — rejected

Removed three temporary Arc reference-count increments/decrements per inserted row: body is borrowed for content hashing; id and uri Arc values are borrowed directly and cloned only for the batch uniqueness sets. Public APIs and source values unchanged. Candidate builds; all 61 exec tests pass, including uniqueness atomicity, hybrid bounds and bulk result elision.

Six alternating process pairs for native and six for durable insert; 24 fresh fixtures per case, one operation. Default embedding, FTS, scalar indexes and durable WAL sync_data preserved. Exact inserted common values validated outside timing. No builds/tests overlapped timing. Fixed case order and uncontrolled host load; no causal significance claimed. Process medians first, paired relative decreases then summarized. Milliseconds below are median of process medians.

| Path | Rows | Lin baseline | Candidate | Paired decrease | Wins | SQLite control decrease |
|---|---:|---:|---:|---:|---:|---:|
| native | 1k | 1.308240 | 1.290448 | 1.54% | 4/6 | -0.11% |
| native | 10k | 13.591563 | 13.422291 | 1.27% | 4/6 | 0.59% |
| durable | 1k | 3.300729 | 3.422531 | -3.26% | 1/6 | 2.78% |
| durable | 10k | 26.206489 | 26.083198 | 0.49% | 5/6 | 0.17% |

Reject: native gains are small (4/6 wins at both sizes), while target durable 1k loses 5/6 pairs with a -3.26% median decrease; unchanged SQLite durable 1k controls improve +2.78%. This does not prove a causal mechanism, but does not justify retention as a performance improvement. Baseline src/exec.rs restored byte-for-byte after saving candidate source and patch. MySQL benchmark changes from the preceding turn preserved. No all-peer victory claimed.


</details>

### 101. 2026-10-03-insert-phase-profile

Артефакты: [2026-10-03-insert-phase-profile](../benches/results/2026-10-03-insert-phase-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-insert-phase-profile/report.md).

# Diagnostic native insert phase profile

Temporary bulk-insert instrumentation on fac9cd1. Three independent processes,
24 fresh one-operation samples per size; each phase records 27 calls per
process (observations plus harness validation/preflight). Fixture setup/drop
excluded. Native macOS ARM host; default embedding, scalar index, FTS and
identity/URI uniqueness retained. No builds/tests overlapped timing.

| Documents | Row build/checks ms | Embedding ms | Scalar index ms | FTS ms | Row maps ms |
|---|---:|---:|---:|---:|---:|
| 1,000 | 0.542542 | 0.400042 | 0.121833 | 0.178125 | 0.036083 |
| 10,000 | 5.677584 | 4.485208 | 1.192791 | 1.750625 | 0.416125 |

Row build includes record_row, default id/hash generation, batch/existing
identity/URI checks, foreign-key checks, and initial row/check-set allocation.
Embedding includes text preparation, batch embedding and insertion of vectors.
Scalar index, FTS and row-map registration are measured individually. Other
work (edge/reserve handling, moving rows, final pack creation and Handle work)
is outside these phase timers. Separate medians must not be summed to estimate
a total median. Host load/cache state and clock/log instrumentation affect
absolute times: these are diagnostic phases, not peer-speed proof.

Row formation is the largest recorded phase. FK checking is a short scan over
the catalog FK list without allocations; moving that check out of the loop
alone is unlikely to address the gap. Repeating the already-rejected
prepare-time sorted BTreeMap construction or shared WAL-vector changes is not
justified by this profile. Further construction work should investigate actual
allocation counts and storage representation while preserving exposed Row/API
behavior, rather than repeat scalar-loop changes without a distinct mechanism.

All three runs are complete with existing fresh-fixture readback/affected-count
gates. Instrumented benchmark builds. Production exec.rs restored byte-for-byte
before running the pinned executable. Source/binary SHA256, source copies,
raw trace logs and observations are retained. Trailing log blank lines normalized;
raw JSON unchanged. Full eight-engine goal remains unproven.


</details>

### 102. 2026-10-03-lex-borrow-lower

Артефакты: [2026-10-03-lex-borrow-lower](../benches/results/2026-10-03-lex-borrow-lower).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-lex-borrow-lower/report.md).

# Borrow lowercase ASCII in lexical scoring

Lexical scoring borrows an already-lowercase ASCII blob instead of allocating
a lowercase copy. Uppercase ASCII and all Unicode still use str::to_lowercase,
including contextual Greek sigma handling. Field concatenation, token/phrase
scoring, top-k ordering and returned rows are unchanged.

Six alternating independent process pairs, 32 warm samples per case, 50ms
warmup, 5ms target, max 100 operations/sample. Same 10k-row fixture, preparation
outside timing. No tests/builds overlap performance. Process medians aggregated
before comparison; host load/cache state uncontrolled.

| Lex query | Baseline µs | Candidate µs | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Common | 159.956 | 145.700 | 8.20% | 6/6 |
| Selective | 0.712 | 0.687 | 3.46% | 5/6 |
| Missing | 0.298 | 0.300 | -1.01% | 2/6 |

The common-term improvement is sustained and exceeds the earlier ~2.2%
FTS-common regression observed with shared AST literals, though measurements
from separate runs are not directly interchangeable. Missing queries never
invoke this scorer; no improvement is claimed for that submicrosecond case.
No peer-speed, native/durable insertion or full eight-engine win is inferred.
SQLite small/durable insertion and MSSQL/Kusto validation remain unresolved.

Focused exec integration tests (61) and baseline/candidate benchmark builds
passed. Added score regression covers lowercase/uppercase ASCII, contextual
Greek sigma and a phrase spanning fields. Full workspace result recorded in
workspace-tests.log. Source/binary hashes and raw samples retained.

Full cargo test --offline --workspace passed. Scoped rustfmt and diff checks
passed. final-exec.rs also records the added regression test; the performance
binary source is candidate-exec.rs. Trailing empty log lines normalized; raw
JSON observations unchanged.


</details>

### 103. 2026-10-03-mongo-native

Артефакты: [2026-10-03-mongo-native](../benches/results/2026-10-03-mongo-native).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/report.md).

# Native MongoDB insert comparison

Three independent processes per size, nine fresh samples per process. Same six
fields, timestamp, unique identity/URI and wing/ts index; exact readback after
every sample. Setup, preparation, readback and cleanup excluded from timing.

| Documents | Lin median ms | MongoDB median ms | Mongo / Lin |
|---|---:|---:|---:|
| 1,000 | 2.668 | 15.226 | 5.71x |
| 10,000 | 20.784 | 100.576 | 4.84x |

Both require-wins runs completed successfully. MongoDB 8.0.28 on native ARM Docker
with tmpfs, ordered PyMongo insert_many, w=1/j=false. Lin uses Db::empty and
includes default embedding/FTS. Successful API insertion only: bulk failure
atomicity and physical disk durability are not equivalent. This does not prove
wins against all eight target engines. Raw process reports are in rows-1000 and
rows-10000. verification.json records binary SHA256, unchanged sentinel data,
and no remaining benchmark databases. The owned Mongo container was stopped.


</details>

<details>
<summary>rows-1000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-1000/process-1/report.md).

# Validated native insert API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 2667.834 | 1.00× | validated |
| insert_native | mongo | 15225.708 | 5.71× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-1000/process-2/report.md).

# Validated native insert API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | mongo | 21702.750 | 7.56× | validated |
| insert_native | lin | 2869.125 | 1.00× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-1000/process-3/report.md).

# Validated native insert API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 2630.291 | 1.00× | validated |
| insert_native | mongo | 13056.667 | 4.96× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-1000/report.md).

# Validated native insert API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 2667.834 | 1.00× | validated |
| insert_native | mongo | 15225.708 | 5.71× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-10000/process-1/report.md).

# Validated native insert API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 22599.625 | 1.00× | validated |
| insert_native | mongo | 103179.292 | 4.57× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-10000/process-2/report.md).

# Validated native insert API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | mongo | 86647.208 | 4.70× | validated |
| insert_native | lin | 18452.959 | 1.00× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-10000/process-3/report.md).

# Validated native insert API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 20784.166 | 1.00× | validated |
| insert_native | mongo | 100575.791 | 4.84× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-mongo-native/rows-10000/report.md).

# Validated native insert API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 20784.166 | 1.00× | validated |
| insert_native | mongo | 100575.791 | 4.84× | validated |

Native contracts: {"lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}}

Versions: {"lin": "0.4.0", "mongo": "8.0.28"}

Missing/failed peers are never counted as wins.


</details>

### 104. 2026-10-03-parallel-row-build

Артефакты: [2026-10-03-parallel-row-build](../benches/results/2026-10-03-parallel-row-build).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-parallel-row-build/report.md).

# Parallel bulk row construction: retained

Baseline: ff5fc07. Candidate builds input Rows before validation, using caller plus one scoped worker at batch sizes >=4096. Smaller batches build rows sequentially. ID allocation, content hash filling, duplicate ID/URI checks, foreign-key checks, embedding and index updates remain serial in original record order. One execution timestamp is shared across both halves; duplicate fields retain last-wins behavior. Public Row representation and wire/durability formats are unchanged.

Six independent alternating baseline/candidate pairs per workload, 24 fresh fixtures per process, one operation/sample and no warmup. Build/tests did not overlap measurement. Benchmark exact affected-count/value/readback checks passed for both batch sizes. Source and binary hashes and all raw run.json samples are saved here; final-exec.rs adds regression tests only after candidate measurement.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | -1.72% | 1/6 | 0.915177 | 0.928885 |
| compare/insert_bulk_1k/sqlite | -1.72% | 2/6 | 0.799615 | 0.813344 |
| compare/insert_bulk_10k/lin | 2.82% | 6/6 | 10.087312 | 9.789990 |
| compare/insert_bulk_10k/sqlite | -0.80% | 1/6 | 10.555396 | 10.583020 |
| compare/durable_insert_1k/lin | 1.21% | 3/6 | 2.502729 | 2.471146 |
| compare/durable_insert_1k/sqlite | 0.67% | 4/6 | 1.429167 | 1.425198 |
| compare/durable_insert_10k/lin | 5.37% | 6/6 | 19.880969 | 18.844136 |
| compare/durable_insert_10k/sqlite | -0.07% | 2/6 | 13.668125 | 13.679312 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Large native and durable workloads improved in all six pairs, while SQLite controls were flat/slower. The small native slowdown closely tracks its SQLite control; small durable results are mixed. These process comparisons do not prove performance on other hardware or document shapes.

Resource tradeoff: the large batch uses two CPU execution contexts rather than one during pure Row construction, then returns to serial execution. Thread creation and join happen per operation and are INCLUDED in insert timing. A temporary vector holds right-half row headers until merge, plus normal OS thread resources. Peak memory/CPU energy were not measured. This is a latency comparison with additional CPU parallelism, not a per-core throughput improvement. The 4096 threshold is a guard against small-batch thread overhead, not a demonstrated universally optimal crossover. All input rows are now constructed before key/FK validation, so invalid input may do extra allocation work before returning its unchanged validation error.

Validation: four existing focused record tests passed. New tests cover 4095/4096/4101 row boundaries, original row order, generated ID sequence, last duplicate fields, Unicode text, generated hashes, URI lookup and one shared execution-time value. Another test checks duplicate ID and URI errors crossing the worker boundary with no stored partial batch. Both new tests passed. Full `cargo test --offline --workspace` passed, including existing durability/reopen/rollback checks. `git diff --check` passed.

Decision: retain the measured large-batch latency improvement. Lin still loses to SQLite on native 1k and durable 1k/10k in these aggregate timings; native 10k favors Lin. MSSQL/Kusto endpoints remain absent. The full nine-engine objective remains incomplete.


</details>

### 105. 2026-10-03-peer-read-refresh

Артефакты: [2026-10-03-peer-read-refresh](../benches/results/2026-10-03-peer-read-refresh).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/report.md).

# Current peer read API matrix

Source commit: e71a17b61dfde8f6b62dd977ba7d0ccc589826f9. Pinned release peer_bench worker.
Three independent processes per size, eight samples per case/process. Six matched
read APIs: point get, equality count, substring count, materialize, inner join,
filtered inner join. Exact result values/multiplicity validated outside timing.
Schema/seeding/preparation and Lin JSON IPC excluded. Python peer timings include
DBAPI/driver/DataFrame work and server round-trip/materialization. This measures
these API paths; it is not an intrinsic engine CPU or universal workload claim.

| Documents | Validated available peer comparisons | Lin aggregate wins | Missing peers |
|---|---:|---:|---|
| 1,000 | 36 | 36 | MSSQL, Kusto |
| 10,000 | 36 | 36 | MSSQL, Kusto |
| 100,000 | 36 | 36 | MSSQL, Kusto |

All 108 available comparisons favor Lin by medians aggregated within each
process first, then across the three processes. Raw batch averages are not
individual latency percentiles. The full gate is INCOMPLETE (exit 2) at every
size because MSSQL/Kusto endpoints are absent; unavailable cases are never wins.

| Peer | Minimum aggregate peer / Lin ratio across 18 read cases |
|---|---:|
| sqlite | 2.93x |
| duckdb | 1.97x |
| postgres | 2.73x |
| mysql | 12.31x |
| mongo | 8.09x |
| pandas | 2.68x |

Dedicated native ARM Docker PostgreSQL/MySQL/MongoDB instances on tmpfs; schema
objects use unique owned names. Sentinel tables/database exact rows remained
unchanged after each dataset. No linbench_ fixture objects remained. All three
owned containers stopped in finally. verification.json records cleanup/sentinels
and binary SHA256; per-dataset reports include actual engine versions.

No builds/tests overlapped timing. Case/engine order rotates across processes;
host load, CPU clocks and server scheduling are uncontrolled. Source/script hashes
and raw reports retained. This read matrix does not erase SQLite small/native
and durable insert losses from the existing write evidence. Native ingestion,
physical disk durability, and MSSQL/Kusto are not verified by this run. The full
eight-engine goal remains unproven.


</details>

<details>
<summary>rows-1000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-1/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.245 | 1.00× | validated |
| point_get | sqlite | 0.961 | 3.93× | validated |
| point_get | duckdb | 39.056 | 159.57× | validated |
| point_get | postgres | 299.033 | 1221.73× | validated |
| point_get | mysql | 366.127 | 1495.85× | validated |
| point_get | mongo | 468.719 | 1914.99× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.557 | 18.62× | validated |
| filter_eq | lin | 0.676 | 1.00× | validated |
| filter_eq | sqlite | 7.822 | 11.57× | validated |
| filter_eq | duckdb | 142.833 | 211.29× | validated |
| filter_eq | postgres | 329.354 | 487.22× | validated |
| filter_eq | mysql | 389.894 | 576.78× | validated |
| filter_eq | mongo | 540.387 | 799.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 64.659 | 95.65× | validated |
| text_substr | lin | 1.877 | 1.00× | validated |
| text_substr | sqlite | 32.344 | 17.23× | validated |
| text_substr | duckdb | 63.543 | 33.85× | validated |
| text_substr | postgres | 353.203 | 188.17× | validated |
| text_substr | mysql | 447.485 | 238.40× | validated |
| text_substr | mongo | 797.733 | 424.99× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 84.515 | 45.03× | validated |
| materialize | lin | 42.726 | 1.00× | validated |
| materialize | sqlite | 174.202 | 4.08× | validated |
| materialize | duckdb | 295.641 | 6.92× | validated |
| materialize | postgres | 431.826 | 10.11× | validated |
| materialize | mysql | 1330.993 | 31.15× | validated |
| materialize | mongo | 1396.722 | 32.69× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 374.163 | 8.76× | validated |
| join_inner | lin | 137.190 | 1.00× | validated |
| join_inner | sqlite | 401.406 | 2.93× | validated |
| join_inner | duckdb | 365.998 | 2.67× | validated |
| join_inner | postgres | 697.316 | 5.08× | validated |
| join_inner | mysql | 2377.323 | 17.33× | validated |
| join_inner | mongo | 12858.375 | 93.73× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 720.476 | 5.25× | validated |
| join_filter | lin | 65.744 | 1.00× | validated |
| join_filter | sqlite | 208.673 | 3.17× | validated |
| join_filter | duckdb | 250.982 | 3.82× | validated |
| join_filter | postgres | 499.690 | 7.60× | validated |
| join_filter | mysql | 1446.854 | 22.01× | validated |
| join_filter | mongo | 8120.167 | 123.51× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 628.917 | 9.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-2/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.958 | 3.77× | validated |
| point_get | duckdb | 41.518 | 163.61× | validated |
| point_get | postgres | 307.967 | 1213.58× | validated |
| point_get | mysql | 375.802 | 1480.90× | validated |
| point_get | mongo | 472.338 | 1861.31× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.546 | 17.92× | validated |
| point_get | lin | 0.254 | 1.00× | validated |
| filter_eq | sqlite | 7.711 | 10.98× | validated |
| filter_eq | duckdb | 142.929 | 203.61× | validated |
| filter_eq | postgres | 348.540 | 496.52× | validated |
| filter_eq | mysql | 387.069 | 551.41× | validated |
| filter_eq | mongo | 563.859 | 803.25× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 59.399 | 84.62× | validated |
| filter_eq | lin | 0.702 | 1.00× | validated |
| text_substr | sqlite | 33.536 | 16.59× | validated |
| text_substr | duckdb | 65.580 | 32.44× | validated |
| text_substr | postgres | 358.109 | 177.15× | validated |
| text_substr | mysql | 434.568 | 214.98× | validated |
| text_substr | mongo | 836.090 | 413.60× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.651 | 38.91× | validated |
| text_substr | lin | 2.021 | 1.00× | validated |
| materialize | sqlite | 172.344 | 4.11× | validated |
| materialize | duckdb | 294.201 | 7.02× | validated |
| materialize | postgres | 445.845 | 10.64× | validated |
| materialize | mysql | 1347.805 | 32.17× | validated |
| materialize | mongo | 1332.924 | 31.82× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 385.125 | 9.19× | validated |
| materialize | lin | 41.892 | 1.00× | validated |
| join_inner | sqlite | 392.904 | 2.93× | validated |
| join_inner | duckdb | 363.107 | 2.71× | validated |
| join_inner | postgres | 680.757 | 5.08× | validated |
| join_inner | mysql | 2382.167 | 17.78× | validated |
| join_inner | mongo | 13618.541 | 101.65× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 751.458 | 5.61× | validated |
| join_inner | lin | 133.974 | 1.00× | validated |
| join_filter | sqlite | 203.528 | 3.02× | validated |
| join_filter | duckdb | 263.043 | 3.90× | validated |
| join_filter | postgres | 541.031 | 8.02× | validated |
| join_filter | mysql | 1368.854 | 20.28× | validated |
| join_filter | mongo | 8346.229 | 123.66× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 667.827 | 9.89× | validated |
| join_filter | lin | 67.496 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-3/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.026 | 159.53× | validated |
| point_get | postgres | 312.443 | 1245.29× | validated |
| point_get | mysql | 352.933 | 1406.67× | validated |
| point_get | mongo | 478.748 | 1908.12× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.829 | 19.25× | validated |
| point_get | lin | 0.251 | 1.00× | validated |
| point_get | sqlite | 0.938 | 3.74× | validated |
| filter_eq | duckdb | 138.738 | 199.78× | validated |
| filter_eq | postgres | 332.556 | 478.86× | validated |
| filter_eq | mysql | 365.732 | 526.64× | validated |
| filter_eq | mongo | 616.321 | 887.47× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 58.545 | 84.30× | validated |
| filter_eq | lin | 0.694 | 1.00× | validated |
| filter_eq | sqlite | 7.663 | 11.03× | validated |
| text_substr | duckdb | 64.037 | 34.37× | validated |
| text_substr | postgres | 357.811 | 192.06× | validated |
| text_substr | mysql | 458.155 | 245.92× | validated |
| text_substr | mongo | 836.562 | 449.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.030 | 41.88× | validated |
| text_substr | lin | 1.863 | 1.00× | validated |
| text_substr | sqlite | 33.005 | 17.72× | validated |
| materialize | duckdb | 297.844 | 7.21× | validated |
| materialize | postgres | 442.286 | 10.70× | validated |
| materialize | mysql | 1393.569 | 33.73× | validated |
| materialize | mongo | 1475.396 | 35.71× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 397.769 | 9.63× | validated |
| materialize | lin | 41.319 | 1.00× | validated |
| materialize | sqlite | 174.858 | 4.23× | validated |
| join_inner | duckdb | 364.548 | 2.64× | validated |
| join_inner | postgres | 695.211 | 5.04× | validated |
| join_inner | mysql | 2586.490 | 18.74× | validated |
| join_inner | mongo | 13833.563 | 100.25× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 758.201 | 5.49× | validated |
| join_inner | lin | 137.990 | 1.00× | validated |
| join_inner | sqlite | 413.634 | 3.00× | validated |
| join_filter | duckdb | 252.616 | 3.67× | validated |
| join_filter | postgres | 516.356 | 7.50× | validated |
| join_filter | mysql | 1552.917 | 22.55× | validated |
| join_filter | mongo | 8679.625 | 126.05× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 673.295 | 9.78× | validated |
| join_filter | lin | 68.858 | 1.00× | validated |
| join_filter | sqlite | 209.153 | 3.04× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-1000/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.251 | 1.00× | validated |
| point_get | sqlite | 0.958 | 3.82× | validated |
| point_get | duckdb | 40.026 | 159.53× | validated |
| point_get | postgres | 307.967 | 1227.45× | validated |
| point_get | mysql | 366.127 | 1459.25× | validated |
| point_get | mongo | 472.338 | 1882.57× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.557 | 18.16× | validated |
| filter_eq | lin | 0.694 | 1.00× | validated |
| filter_eq | sqlite | 7.711 | 11.10× | validated |
| filter_eq | duckdb | 142.833 | 205.67× | validated |
| filter_eq | postgres | 332.556 | 478.86× | validated |
| filter_eq | mysql | 387.069 | 557.36× | validated |
| filter_eq | mongo | 563.859 | 811.93× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 59.399 | 85.53× | validated |
| text_substr | lin | 1.877 | 1.00× | validated |
| text_substr | sqlite | 33.005 | 17.58× | validated |
| text_substr | duckdb | 64.037 | 34.12× | validated |
| text_substr | postgres | 357.811 | 190.62× | validated |
| text_substr | mysql | 447.485 | 238.40× | validated |
| text_substr | mongo | 836.090 | 445.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.651 | 41.90× | validated |
| materialize | lin | 41.892 | 1.00× | validated |
| materialize | sqlite | 174.202 | 4.16× | validated |
| materialize | duckdb | 295.641 | 7.06× | validated |
| materialize | postgres | 442.286 | 10.56× | validated |
| materialize | mysql | 1347.805 | 32.17× | validated |
| materialize | mongo | 1396.722 | 33.34× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 385.125 | 9.19× | validated |
| join_inner | lin | 137.190 | 1.00× | validated |
| join_inner | sqlite | 401.406 | 2.93× | validated |
| join_inner | duckdb | 364.548 | 2.66× | validated |
| join_inner | postgres | 695.211 | 5.07× | validated |
| join_inner | mysql | 2382.167 | 17.36× | validated |
| join_inner | mongo | 13618.541 | 99.27× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 751.458 | 5.48× | validated |
| join_filter | lin | 67.496 | 1.00× | validated |
| join_filter | sqlite | 208.673 | 3.09× | validated |
| join_filter | duckdb | 252.616 | 3.74× | validated |
| join_filter | postgres | 516.356 | 7.65× | validated |
| join_filter | mysql | 1446.854 | 21.44× | validated |
| join_filter | mongo | 8346.229 | 123.66× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 667.827 | 9.89× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-1/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.241 | 1.00× | validated |
| point_get | sqlite | 0.992 | 4.12× | validated |
| point_get | duckdb | 39.668 | 164.59× | validated |
| point_get | postgres | 271.038 | 1124.58× | validated |
| point_get | mysql | 346.728 | 1438.63× | validated |
| point_get | mongo | 517.305 | 2146.38× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.673 | 19.39× | validated |
| filter_eq | lin | 0.683 | 1.00× | validated |
| filter_eq | sqlite | 69.269 | 101.47× | validated |
| filter_eq | duckdb | 192.169 | 281.51× | validated |
| filter_eq | postgres | 495.923 | 726.49× | validated |
| filter_eq | mysql | 681.857 | 998.87× | validated |
| filter_eq | mongo | 932.552 | 1366.12× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 275.618 | 403.76× | validated |
| text_substr | lin | 17.382 | 1.00× | validated |
| text_substr | sqlite | 334.554 | 19.25× | validated |
| text_substr | duckdb | 104.490 | 6.01× | validated |
| text_substr | postgres | 789.260 | 45.41× | validated |
| text_substr | mysql | 1266.174 | 72.84× | validated |
| text_substr | mongo | 3056.229 | 175.83× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 506.891 | 29.16× | validated |
| materialize | lin | 421.597 | 1.00× | validated |
| materialize | sqlite | 1946.333 | 4.62× | validated |
| materialize | duckdb | 1266.812 | 3.00× | validated |
| materialize | postgres | 1694.677 | 4.02× | validated |
| materialize | mysql | 10057.834 | 23.86× | validated |
| materialize | mongo | 5859.083 | 13.90× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1888.542 | 4.48× | validated |
| join_inner | lin | 1350.562 | 1.00× | validated |
| join_inner | sqlite | 4597.520 | 3.40× | validated |
| join_inner | duckdb | 2725.084 | 2.02× | validated |
| join_inner | postgres | 3834.291 | 2.84× | validated |
| join_inner | mysql | 21546.938 | 15.95× | validated |
| join_inner | mongo | 120274.895 | 89.06× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 4075.229 | 3.02× | validated |
| join_filter | lin | 663.809 | 1.00× | validated |
| join_filter | sqlite | 2344.354 | 3.53× | validated |
| join_filter | duckdb | 1482.681 | 2.23× | validated |
| join_filter | postgres | 2201.198 | 3.32× | validated |
| join_filter | mysql | 11498.834 | 17.32× | validated |
| join_filter | mongo | 61307.959 | 92.36× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2332.010 | 3.51× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-2/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.955 | 3.75× | validated |
| point_get | duckdb | 39.972 | 156.83× | validated |
| point_get | postgres | 265.194 | 1040.51× | validated |
| point_get | mysql | 309.344 | 1213.74× | validated |
| point_get | mongo | 482.825 | 1894.41× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.569 | 17.93× | validated |
| point_get | lin | 0.255 | 1.00× | validated |
| filter_eq | sqlite | 69.685 | 88.56× | validated |
| filter_eq | duckdb | 181.979 | 231.26× | validated |
| filter_eq | postgres | 492.956 | 626.46× | validated |
| filter_eq | mysql | 655.090 | 832.50× | validated |
| filter_eq | mongo | 810.750 | 1030.31× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 255.448 | 324.63× | validated |
| filter_eq | lin | 0.787 | 1.00× | validated |
| text_substr | sqlite | 341.787 | 18.17× | validated |
| text_substr | duckdb | 95.287 | 5.06× | validated |
| text_substr | postgres | 765.830 | 40.70× | validated |
| text_substr | mysql | 1275.174 | 67.77× | validated |
| text_substr | mongo | 2851.562 | 151.56× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 492.961 | 26.20× | validated |
| text_substr | lin | 18.815 | 1.00× | validated |
| materialize | sqlite | 1908.386 | 4.23× | validated |
| materialize | duckdb | 1243.055 | 2.75× | validated |
| materialize | postgres | 1620.195 | 3.59× | validated |
| materialize | mysql | 10036.646 | 22.23× | validated |
| materialize | mongo | 5355.125 | 11.86× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1824.614 | 4.04× | validated |
| materialize | lin | 451.396 | 1.00× | validated |
| join_inner | sqlite | 4556.480 | 3.14× | validated |
| join_inner | duckdb | 2594.396 | 1.79× | validated |
| join_inner | postgres | 3947.146 | 2.72× | validated |
| join_inner | mysql | 21539.188 | 14.86× | validated |
| join_inner | mongo | 109530.625 | 75.56× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3976.187 | 2.74× | validated |
| join_inner | lin | 1449.528 | 1.00× | validated |
| join_filter | sqlite | 2327.042 | 3.19× | validated |
| join_filter | duckdb | 1401.764 | 1.92× | validated |
| join_filter | postgres | 2270.271 | 3.11× | validated |
| join_filter | mysql | 11149.229 | 15.29× | validated |
| join_filter | mongo | 54645.395 | 74.92× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2325.938 | 3.19× | validated |
| join_filter | lin | 729.389 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-3/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 41.322 | 167.23× | validated |
| point_get | postgres | 271.838 | 1100.13× | validated |
| point_get | mysql | 290.321 | 1174.93× | validated |
| point_get | mongo | 448.197 | 1813.86× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.417 | 17.87× | validated |
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 0.965 | 3.90× | validated |
| filter_eq | duckdb | 180.150 | 263.32× | validated |
| filter_eq | postgres | 487.971 | 713.25× | validated |
| filter_eq | mysql | 606.417 | 886.38× | validated |
| filter_eq | mongo | 806.808 | 1179.28× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 262.681 | 383.95× | validated |
| filter_eq | lin | 0.684 | 1.00× | validated |
| filter_eq | sqlite | 69.966 | 102.27× | validated |
| text_substr | duckdb | 98.647 | 5.70× | validated |
| text_substr | postgres | 778.479 | 44.95× | validated |
| text_substr | mysql | 1263.385 | 72.95× | validated |
| text_substr | mongo | 2880.521 | 166.34× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 503.404 | 29.07× | validated |
| text_substr | lin | 17.318 | 1.00× | validated |
| text_substr | sqlite | 331.234 | 19.13× | validated |
| materialize | duckdb | 1235.495 | 3.02× | validated |
| materialize | postgres | 1683.458 | 4.12× | validated |
| materialize | mysql | 9672.250 | 23.67× | validated |
| materialize | mongo | 5239.896 | 12.82× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1822.208 | 4.46× | validated |
| materialize | lin | 408.611 | 1.00× | validated |
| materialize | sqlite | 1983.177 | 4.85× | validated |
| join_inner | duckdb | 2679.604 | 1.97× | validated |
| join_inner | postgres | 3860.562 | 2.84× | validated |
| join_inner | mysql | 20731.375 | 15.27× | validated |
| join_inner | mongo | 113076.687 | 83.28× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3952.438 | 2.91× | validated |
| join_inner | lin | 1357.840 | 1.00× | validated |
| join_inner | sqlite | 4529.249 | 3.34× | validated |
| join_filter | duckdb | 1423.611 | 2.18× | validated |
| join_filter | postgres | 2214.427 | 3.39× | validated |
| join_filter | mysql | 10877.062 | 16.67× | validated |
| join_filter | mongo | 56145.438 | 86.03× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2421.271 | 3.71× | validated |
| join_filter | lin | 652.637 | 1.00× | validated |
| join_filter | sqlite | 2362.667 | 3.62× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-10000/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 0.965 | 3.90× | validated |
| point_get | duckdb | 39.972 | 161.77× | validated |
| point_get | postgres | 271.038 | 1096.89× | validated |
| point_get | mysql | 309.344 | 1251.92× | validated |
| point_get | mongo | 482.825 | 1954.00× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.569 | 18.49× | validated |
| filter_eq | lin | 0.684 | 1.00× | validated |
| filter_eq | sqlite | 69.685 | 101.86× | validated |
| filter_eq | duckdb | 181.979 | 265.99× | validated |
| filter_eq | postgres | 492.956 | 720.54× | validated |
| filter_eq | mysql | 655.090 | 957.52× | validated |
| filter_eq | mongo | 810.750 | 1185.04× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 262.681 | 383.95× | validated |
| text_substr | lin | 17.382 | 1.00× | validated |
| text_substr | sqlite | 334.554 | 19.25× | validated |
| text_substr | duckdb | 98.647 | 5.68× | validated |
| text_substr | postgres | 778.479 | 44.79× | validated |
| text_substr | mysql | 1266.174 | 72.84× | validated |
| text_substr | mongo | 2880.521 | 165.72× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 503.404 | 28.96× | validated |
| materialize | lin | 421.597 | 1.00× | validated |
| materialize | sqlite | 1946.333 | 4.62× | validated |
| materialize | duckdb | 1243.055 | 2.95× | validated |
| materialize | postgres | 1683.458 | 3.99× | validated |
| materialize | mysql | 10036.646 | 23.81× | validated |
| materialize | mongo | 5355.125 | 12.70× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1824.614 | 4.33× | validated |
| join_inner | lin | 1357.840 | 1.00× | validated |
| join_inner | sqlite | 4556.480 | 3.36× | validated |
| join_inner | duckdb | 2679.604 | 1.97× | validated |
| join_inner | postgres | 3860.562 | 2.84× | validated |
| join_inner | mysql | 21539.188 | 15.86× | validated |
| join_inner | mongo | 113076.687 | 83.28× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3976.187 | 2.93× | validated |
| join_filter | lin | 663.809 | 1.00× | validated |
| join_filter | sqlite | 2344.354 | 3.53× | validated |
| join_filter | duckdb | 1423.611 | 2.14× | validated |
| join_filter | postgres | 2214.427 | 3.34× | validated |
| join_filter | mysql | 11149.229 | 16.80× | validated |
| join_filter | mongo | 56145.438 | 84.58× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2332.010 | 3.51× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.244 | 1.00× | validated |
| point_get | sqlite | 1.013 | 4.14× | validated |
| point_get | duckdb | 39.973 | 163.56× | validated |
| point_get | postgres | 280.072 | 1146.00× | validated |
| point_get | mysql | 349.969 | 1432.01× | validated |
| point_get | mongo | 432.362 | 1769.15× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.401 | 18.01× | validated |
| filter_eq | lin | 0.738 | 1.00× | validated |
| filter_eq | sqlite | 700.128 | 948.77× | validated |
| filter_eq | duckdb | 438.163 | 593.77× | validated |
| filter_eq | postgres | 2611.250 | 3538.61× | validated |
| filter_eq | mysql | 3354.959 | 4546.44× | validated |
| filter_eq | mongo | 3913.541 | 5303.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2329.406 | 3156.67× | validated |
| text_substr | lin | 208.178 | 1.00× | validated |
| text_substr | sqlite | 3327.271 | 15.98× | validated |
| text_substr | duckdb | 470.685 | 2.26× | validated |
| text_substr | postgres | 5268.812 | 25.31× | validated |
| text_substr | mysql | 9586.312 | 46.05× | validated |
| text_substr | mongo | 23570.187 | 113.22× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4469.500 | 21.47× | validated |
| materialize | lin | 5363.209 | 1.00× | validated |
| materialize | sqlite | 20274.854 | 3.78× | validated |
| materialize | duckdb | 12346.834 | 2.30× | validated |
| materialize | postgres | 15143.000 | 2.82× | validated |
| materialize | mysql | 87109.583 | 16.24× | validated |
| materialize | mongo | 44764.583 | 8.35× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17064.625 | 3.18× | validated |
| join_inner | lin | 14019.188 | 1.00× | validated |
| join_inner | sqlite | 51422.916 | 3.67× | validated |
| join_inner | duckdb | 29679.395 | 2.12× | validated |
| join_inner | postgres | 40420.854 | 2.88× | validated |
| join_inner | mysql | 173184.416 | 12.35× | validated |
| join_inner | mongo | 1124911.250 | 80.24× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 36313.875 | 2.59× | validated |
| join_filter | lin | 7046.855 | 1.00× | validated |
| join_filter | sqlite | 25415.312 | 3.61× | validated |
| join_filter | duckdb | 14270.688 | 2.03× | validated |
| join_filter | postgres | 21915.646 | 3.11× | validated |
| join_filter | mysql | 93154.583 | 13.22× | validated |
| join_filter | mongo | 561053.250 | 79.62× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 18780.854 | 2.67× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.982 | 2.21× | validated |
| point_get | duckdb | 40.013 | 90.13× | validated |
| point_get | postgres | 271.647 | 611.85× | validated |
| point_get | mysql | 366.962 | 826.54× | validated |
| point_get | mongo | 444.757 | 1001.77× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.710 | 10.61× | validated |
| point_get | lin | 0.444 | 1.00× | validated |
| filter_eq | sqlite | 705.976 | 827.39× | validated |
| filter_eq | duckdb | 416.713 | 488.38× | validated |
| filter_eq | postgres | 2329.958 | 2730.66× | validated |
| filter_eq | mysql | 3440.646 | 4032.35× | validated |
| filter_eq | mongo | 3792.521 | 4444.74× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2318.792 | 2717.57× | validated |
| filter_eq | lin | 0.853 | 1.00× | validated |
| text_substr | sqlite | 3364.834 | 15.07× | validated |
| text_substr | duckdb | 465.071 | 2.08× | validated |
| text_substr | postgres | 5248.146 | 23.51× | validated |
| text_substr | mysql | 9826.750 | 44.01× | validated |
| text_substr | mongo | 24165.209 | 108.24× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5165.104 | 23.13× | validated |
| text_substr | lin | 223.264 | 1.00× | validated |
| materialize | sqlite | 20899.688 | 3.76× | validated |
| materialize | duckdb | 12406.896 | 2.23× | validated |
| materialize | postgres | 14948.791 | 2.69× | validated |
| materialize | mysql | 85180.730 | 15.33× | validated |
| materialize | mongo | 45214.854 | 8.14× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17799.480 | 3.20× | validated |
| materialize | lin | 5556.021 | 1.00× | validated |
| join_inner | sqlite | 52371.542 | 1.93× | validated |
| join_inner | duckdb | 28781.354 | 1.06× | validated |
| join_inner | postgres | 39920.084 | 1.47× | validated |
| join_inner | mysql | 174297.729 | 6.42× | validated |
| join_inner | mongo | 1113695.729 | 41.03× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38287.854 | 1.41× | validated |
| join_inner | lin | 27143.729 | 1.00× | validated |
| join_filter | sqlite | 26031.896 | 3.43× | validated |
| join_filter | duckdb | 14055.542 | 1.85× | validated |
| join_filter | postgres | 21602.105 | 2.85× | validated |
| join_filter | mysql | 94459.042 | 12.45× | validated |
| join_filter | mongo | 571808.812 | 75.36× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19478.375 | 2.57× | validated |
| join_filter | lin | 7587.500 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 42.616 | 173.57× | validated |
| point_get | postgres | 290.336 | 1182.49× | validated |
| point_get | mysql | 352.598 | 1436.07× | validated |
| point_get | mongo | 474.523 | 1932.66× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.669 | 19.02× | validated |
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.036 | 4.22× | validated |
| filter_eq | duckdb | 442.046 | 649.97× | validated |
| filter_eq | postgres | 2466.083 | 3626.04× | validated |
| filter_eq | mysql | 3368.625 | 4953.10× | validated |
| filter_eq | mongo | 3901.834 | 5737.11× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2285.208 | 3360.08× | validated |
| filter_eq | lin | 0.680 | 1.00× | validated |
| filter_eq | sqlite | 701.903 | 1032.05× | validated |
| text_substr | duckdb | 491.975 | 2.39× | validated |
| text_substr | postgres | 5511.667 | 26.82× | validated |
| text_substr | mysql | 9528.646 | 46.37× | validated |
| text_substr | mongo | 23740.541 | 115.53× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4845.458 | 23.58× | validated |
| text_substr | lin | 205.496 | 1.00× | validated |
| text_substr | sqlite | 3477.812 | 16.92× | validated |
| materialize | duckdb | 13296.750 | 2.38× | validated |
| materialize | postgres | 18320.354 | 3.28× | validated |
| materialize | mysql | 93094.854 | 16.67× | validated |
| materialize | mongo | 44930.854 | 8.04× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 18209.896 | 3.26× | validated |
| materialize | lin | 5586.020 | 1.00× | validated |
| materialize | sqlite | 21216.666 | 3.80× | validated |
| join_inner | duckdb | 30561.500 | 2.16× | validated |
| join_inner | postgres | 39464.917 | 2.79× | validated |
| join_inner | mysql | 174468.229 | 12.32× | validated |
| join_inner | mongo | 1132299.479 | 79.96× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38015.438 | 2.68× | validated |
| join_inner | lin | 14159.959 | 1.00× | validated |
| join_inner | sqlite | 51937.834 | 3.67× | validated |
| join_filter | duckdb | 14593.312 | 2.06× | validated |
| join_filter | postgres | 21661.791 | 3.06× | validated |
| join_filter | mysql | 93329.833 | 13.19× | validated |
| join_filter | mongo | 588559.688 | 83.15× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19719.062 | 2.79× | validated |
| join_filter | lin | 7077.958 | 1.00× | validated |
| join_filter | sqlite | 26600.229 | 3.76× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-peer-read-refresh/rows-100000/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.013 | 4.12× | validated |
| point_get | duckdb | 40.013 | 162.97× | validated |
| point_get | postgres | 280.072 | 1140.69× | validated |
| point_get | mysql | 352.598 | 1436.07× | validated |
| point_get | mongo | 444.757 | 1811.42× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.669 | 19.02× | validated |
| filter_eq | lin | 0.738 | 1.00× | validated |
| filter_eq | sqlite | 701.903 | 951.18× | validated |
| filter_eq | duckdb | 438.163 | 593.77× | validated |
| filter_eq | postgres | 2466.083 | 3341.89× | validated |
| filter_eq | mysql | 3368.625 | 4564.96× | validated |
| filter_eq | mongo | 3901.834 | 5287.54× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2318.792 | 3142.29× | validated |
| text_substr | lin | 208.178 | 1.00× | validated |
| text_substr | sqlite | 3364.834 | 16.16× | validated |
| text_substr | duckdb | 470.685 | 2.26× | validated |
| text_substr | postgres | 5268.812 | 25.31× | validated |
| text_substr | mysql | 9586.312 | 46.05× | validated |
| text_substr | mongo | 23740.541 | 114.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4845.458 | 23.28× | validated |
| materialize | lin | 5556.021 | 1.00× | validated |
| materialize | sqlite | 20899.688 | 3.76× | validated |
| materialize | duckdb | 12406.896 | 2.23× | validated |
| materialize | postgres | 15143.000 | 2.73× | validated |
| materialize | mysql | 87109.583 | 15.68× | validated |
| materialize | mongo | 44930.854 | 8.09× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17799.480 | 3.20× | validated |
| join_inner | lin | 14159.959 | 1.00× | validated |
| join_inner | sqlite | 51937.834 | 3.67× | validated |
| join_inner | duckdb | 29679.395 | 2.10× | validated |
| join_inner | postgres | 39920.084 | 2.82× | validated |
| join_inner | mysql | 174297.729 | 12.31× | validated |
| join_inner | mongo | 1124911.250 | 79.44× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38015.438 | 2.68× | validated |
| join_filter | lin | 7077.958 | 1.00× | validated |
| join_filter | sqlite | 26031.896 | 3.68× | validated |
| join_filter | duckdb | 14270.688 | 2.02× | validated |
| join_filter | postgres | 21661.791 | 3.06× | validated |
| join_filter | mysql | 93329.833 | 13.19× | validated |
| join_filter | mongo | 571808.812 | 80.79× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19478.375 | 2.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

### 106. 2026-10-03-shared-ast-strings

Артефакты: [2026-10-03-shared-ast-strings](../benches/results/2026-10-03-shared-ast-strings).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-shared-ast-strings/report.md).

# Shared AST string literals

Retained change: experimental ast::Value::String stores Arc<str> instead of
String. Parser/fluent constructors own shared literals; value_cell and index
literal conversion clone their Arc rather than allocate another text payload.
Stable Row remains BTreeMap<String, Cell>, Cell remains unchanged. Relative
execution-time values, duplicate-field last-write semantics and source/plan
rendering remain covered by workspace tests. This is distinct from the previously
rejected prepared side cache: no extra per-record cache or lookup is retained.

Compatibility: manual experimental AST constructors that pass an owned String
must use Value::String(text.into()). Constructors already using .into() and
stable text/query APIs continue to work. Experimental AST/Stmt/plan IR are outside
the stable API freeze documented in src/lib.rs. No CLI or WAL format change.

Six alternating independent process pairs per write mode, 24 fresh fixtures per
case, one timed operation, no warmup. Default embedding, FTS, scalar indexes,
identity/URI constraints and Full sync retained. Exact readback/affected-count
gates passed. Preparation/schema/validation/drop excluded according to existing
case contracts. Native Handle output drop remains inside timing; durable Handle
drop remains outside timing. No builds/tests overlap performance runs. Host load
and filesystem scheduling uncontrolled. Aggregate process medians, not pooled
operation latency; paired gain is median of per-pair relative changes.

| Lin case | Baseline ms | Candidate ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Native 1k | 1.318157 | 1.244375 | 5.25% | 6/6 |
| Native 10k | 14.004823 | 13.215157 | 5.63% | 6/6 |
| Durable 1k | 3.260698 | 3.123625 | 1.44% | 3/6 |
| Durable 10k | 25.790479 | 24.796844 | 3.30% | 6/6 |

Small durable improvement is inconclusive. SQLite controls: native 1k -0.53%,
native 10k -0.01%, durable 1k -1.67%, durable 10k -0.75% paired gain. Candidate
Lin beats SQLite native 10k in 6/6 processes; loses native 1k and both durable
sizes in 6/6. This does not prove the full eight-engine objective.

Read checks: three alternating independent pairs, 32 warm samples, 50ms warmup,
5ms target, max 100 operations/sample. FTS concern expanded to six pairs.

| Lin read | Paired gain | Wins |
|---|---:|---:|
| Point get | 0.00% | 1/3 (one tie) |
| Equality filter | 6.89% | 3/3 |
| Range filter | 9.64% | 3/3 |
| Text substring | 4.82% | 3/3 |
| Materialize | 0.70% | 3/3 |
| FTS common, expanded six pairs | -2.19% | 2/6 |

Known tradeoff: FTS common is slower in four of six pairs. The mechanism is not
established by wall-clock measurements. The change is retained for consistent
native/large durable insertion and filter gains; FTS remains an open performance
issue, and no universal speedup is claimed. Submicrosecond/very small changes
should be treated cautiously. Read results are before/after Lin checks, not
refreshed peer-engine wins. MSSQL/Kusto endpoints remain unavailable.

Validation: cargo check --offline --all-targets, full cargo test --offline
--workspace, cargo check --offline --features gpu --all-targets passed. New
regression verifies Unicode literal storage is shared with the prepared AST and
survives source/plan destruction. GPU runtime performance is not measured.
Baseline/candidate benchmark builds pass; source/binary SHA256, source copies,
raw observations and process medians retained. Trailing log blank lines normalized;
raw JSON unchanged. Scoped rustfmt and source diff checks pass.


</details>

### 107. 2026-10-03-wal-block-mask

Артефакты: [2026-10-03-wal-block-mask](../benches/results/2026-10-03-wal-block-mask).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-wal-block-mask/report.md).

# Sparse WAL block masks: rejected

Baseline: 2651115. Candidate replaces the eight-element bitwise-OR empty-block check followed by conditional element traversal with an eight-bit presence mask and trailing-zero traversal. Negative zero and all NaN payloads remain present, ascending index order is preserved, and wire format/sync guarantees are unchanged. Saved sources and binary hashes identify the actual measured experiment.

Six alternating independent baseline/candidate process pairs; 24 fresh fixtures per process, one operation/sample, no warmup. Setup/schema/preparation excluded; synchronous operation including durability flush measured. Exact fixture/readback checks passed. SQLite controls measured alongside Lin. Compilation/tests did not overlap measurement.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/durable_insert_1k/lin | -1.69% | 2/6 | 2.520719 | 2.608083 |
| compare/durable_insert_1k/sqlite | 1.27% | 4/6 | 1.423688 | 1.419823 |
| compare/durable_insert_10k/lin | 0.65% | 4/6 | 19.073958 | 19.005094 |
| compare/durable_insert_10k/sqlite | 0.94% | 4/6 | 14.163094 | 14.029719 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). The small durable workload regressed; the larger workload gain is weaker than SQLite control movement. Decision: reject. src/persist.rs was restored byte-for-byte to baseline. This experiment does not close the existing durable-write gap against SQLite.

Validation: two focused sparse-vector library tests passed, covering bit-preserving -0/NaN/Inf values, missing/empty vectors, block boundaries, partial tail, dense fallback, checksums, truncation, and malformed dimensions/indices/trailing bytes. Both benchmark binaries built. Full workspace tests were not run for this rejected candidate.

First launches waited in macOS dyld before application entry; first baseline startup stack was captured in startup.sample. The existing live benchmark handle was retained throughout, without duplicate/restarted measurement processes. Startup delay did not enter benchmark samples. All twelve benchmark processes finished successfully with complete reports. Raw samples are retained; batch averages are not independent process replications or latency percentiles.

MSSQL/Kusto endpoint variables remain absent. The full nine-engine goal remains incomplete, independent of this rejected local optimization.


</details>

### 108. 2026-10-03-wal-cached-counts

Артефакты: [2026-10-03-wal-cached-counts](../benches/results/2026-10-03-wal-cached-counts).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-wal-cached-counts/report.md).

# Rejected sparse WAL cached-count experiment

Cached each row's nonzero count while choosing sparse codec, then reused counts
while serializing. This removed a vector scan but added one temporary Vec<usize>
per vector column. Frame representation, sparse selection thresholds, decoded
bounds and Full sync were unchanged. Persistence bit-preservation/checksum and
truncation unit tests passed (6). Baseline/candidate benchmark builds passed.

Six alternating independent process pairs, 24 fresh fixtures per case, one
operation per sample, no warmup. Setup/preparation/validation/drop excluded;
existing readback/count gates passed. No builds/tests overlapped timing. Host
load and filesystem scheduling are uncontrolled. Aggregate process medians;
paired gain is the median of six per-pair relative changes.

| Durable case | Baseline ms | Candidate ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Lin 1k | 3.233135 | 3.448479 | -5.14% | 1/6 |
| Lin 10k | 25.070302 | 24.401886 | 3.38% | 4/6 |
| SQLite 1k control | 1.785250 | 1.773094 | 0.46% | 4/6 |
| SQLite 10k control | 18.052771 | 18.040709 | 0.43% | 5/6 |

Rejected: small durable insertion regressed substantially with a comparatively
stable SQLite control. The larger-batch gain was less consistent than the
retained zero-block optimization. These wall-clock results do not establish
the mechanism of the small-batch regression. Production persist.rs restored
byte-for-byte to 9462842, retaining the previously validated zero-block path.
Raw observations, source/binary SHA256 and candidate source are retained.
Trailing blank log lines normalized; raw JSON observations unchanged.

The full eight-engine goal remains unproven: SQLite insertion still wins and
MSSQL/Kusto test endpoints are unavailable. Removing repeated vector scans alone
has not yielded a stable small-batch improvement; broader insertion cost or
allocation changes need independent evidence before retention.


</details>

### 109. 2026-10-03-wal-profile

Артефакты: [2026-10-03-wal-profile](../benches/results/2026-10-03-wal-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-wal-profile/report.md).

# Diagnostic WAL phase profile

Temporary instrumentation on baseline cc5008b separates column encoding,
checksum/header construction, four write_all calls, and durable_sync. Native
macOS ARM host; Full sync preserved. Three independent processes, 24 fresh
one-operation samples per size. Each phase includes 27 events per process
(the 24 observations plus harness validation/preflight calls). Medians within
each process, then median of process medians. No builds/tests overlap timing.

| Documents | Payload bytes | Encode ms | Checksum ms | Write ms | Sync ms |
|---|---:|---:|---:|---:|---:|
| 1,000 | 336,968 | 0.532375 | 0.011667 | 0.045875 | 0.882709 |
| 10,000 | 3,558,340 | 5.662250 | 0.114958 | 0.273541 | 1.341334 |

Diagnostic measurements include clock reads and stderr trace logging between
stages. They are not clean before/after performance proof or engine comparison.
Separate phase medians must not be added to estimate a median total. Host load,
filesystem scheduling and cache state are uncontrolled. The measured write
stage is much smaller than encode/sync; batching syscalls is unlikely to close
the SQLite gap by itself. Sparse vector encoding scales substantially with size
and should be the next profiling/optimization target. Durability is unchanged.

The instrumented benchmark builds and all three runs are complete with the
existing fresh-fixture affected-count/readback gates. Production persist.rs was
restored byte-for-byte before running the pinned diagnostic executable; no
profiling code is retained in production. Raw traces, run.json, source copies,
and binary/source hashes are retained. The full eight-engine goal is unproven.


</details>

### 110. 2026-10-03-wal-zero-blocks

Артефакты: [2026-10-03-wal-zero-blocks](../benches/results/2026-10-03-wal-zero-blocks).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-03-wal-zero-blocks/report.md).

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


</details>

### 111. 2026-10-04-batch-token-slots

Артефакты: [2026-10-04-batch-token-slots](../benches/results/2026-10-04-batch-token-slots).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-batch-token-slots/report.md).

# Rejected bounded batch token-slot cache

Baseline main 4926d9c. Candidate: for HashingEmbedder batches of at least 128 texts, keep up to 32 ASCII alphabetic tokens of byte length 3..24 in a per-batch FxHashMap. Cache the exact ordered unigram/trigram slot sequence, including repeated slots, instead of aggregated weights. On each occurrence apply weight 1 to the unigram slot, then 0.5 to every trigram slot in the original order. Single embedding, numbers, shorter/longer and Unicode tokens keep the existing hashing path. Whole-string bigrams and normalization are unchanged. Cache contents do not outlive a batch or cross embedder dimensions. This adds bounded maps/keys/slot arrays and lookups; no CPU worker or dependency is added.

## Native and durable comparison

Two independent series using the same pinned baseline/candidate binaries, each six alternating process pairs in both modes. Each case has 24 fresh checked fixtures, one operation/sample, no warmup. Existing row/count validation is outside timing. Builds/tests did not overlap measurements. SQLite is an unchanged control. Percentages are medians of the six paired percentage changes from per-process medians, not aggregate-median ratios. CPU frequency/background load are uncontrolled.

| Series/mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series Native | +1.093%, 3/6 | -0.303%, 3/6 | +11.699%, 5/6 | +1.605%, 4/6 |
| First series Durable | -2.152%, 3/6 | +0.282%, 4/6 | -5.075%, 3/6 | -7.529%, 2/6 |
| Independent repeat Native | -0.699%, 1/6 | -0.475%, 2/6 | -1.266%, 0/6 | -0.151%, 3/6 |
| Independent repeat Durable | -4.336%, 2/6 | +0.739%, 4/6 | +0.609%, 6/6 | +1.088%, 5/6 |

The initial native 10k gain does not reproduce: the same candidate loses every native 10k pair in the repeat. The first durable 10k series has large unchanged-control slowdown, so its result is not an isolated causal estimate. Durable 1k is negative in both series, with a repeat 4.336% slowdown against a slightly improving SQLite control. Reject the cache: exact bit compatibility does not establish a useful or consistently faster implementation. No assembler/profile attribution or universal caching claim is made.

## Validation and restoration

All seven focused embedding tests passed. New coverage asserts the 32-entry cap after 64 distinct eligible tokens; raw accumulator bits versus original ordered token updates; the 127/128/131 batch boundary; exact output length/dimension and float bits against individual original embedding; dimensions 8/9/768/1536; repeated eligible tokens, empty strings, Greek contextual lowercasing and other Unicode, ASCII vertical tab, Unicode whitespace, numeric and too-long tokens. Existing independent dense-reference, normalization-boundary and exhaustive bigram hash tests passed. Full workspace was not rerun because the production candidate and new tests were restored byte-for-byte to before-embed.rs.

Baseline binary SHA256 matches the retained sparse-classifier candidate metadata. Raw results, source/binary hashes, test/build logs and invocation scripts identify both measurements. Prior confirmed WAL improvements remain in production, including the small local durable-10k lead over SQLite; durable/native 1k and full named-peer proof remain unresolved. The goal stays active.


</details>

### 112. 2026-10-04-current-insert-profile

Артефакты: [2026-10-04-current-insert-profile](../benches/results/2026-10-04-current-insert-profile).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-insert-profile/report.md).

# Current insert diagnostic phase profile

Source: 703bac5. Three independent processes, 24 fresh one-operation samples per size plus fixture validation/preflight: 27 phase events per size/process. Native macOS ARM, default embedding and scalar/FTS/identity/URI indexes retained. Builds/tests did not overlap measurement. Exact count/value fixture checks passed in all three complete runs.

| Documents | Row construction ms | Hash/ID/checks ms | Embedding ms | Scalar index ms | FTS ms | Row maps ms |
|---|---:|---:|---:|---:|---:|---:|
| 1000 | 0.207084 | 0.110208 | 0.298292 | 0.093417 | 0.134166 | 0.040458 |
| 10000 | 1.446250 | 1.313625 | 3.435333 | 0.919959 | 1.391167 | 0.558958 |

Construction includes initial row-vector allocation and (10k) caller+worker construction/join. Checks include batch-ID/URI set allocation/reserve, missing-ID/default-hash filling, duplicate/existing ID/URI and FK validation. Embedding includes text preparation, default batch embedding and vector insertion into rows. Remaining indexed phases are timed separately. Edge work, store reserve, moving rows, pack creation and Handle finalization are outside these timers.

These are medians within each process, then medians across three processes. Do not sum separate medians into a total or compare absolute times against independently timed peer benchmarks. Diagnostic clocks/logging, cache/load/CPU frequencies and compile changes may affect values. The older 2026-10-03 profile combined construction and checks; that column is not directly interchangeable with these separated columns. This is not proof of any new speedup or intrinsic CPU causality.

Embedding is the largest measured phase at both sizes. Next distinct experiment should examine repeated byte-trigram hashing while preserving exact vector bits and arbitrary-dimension/Unicode fallbacks. Lower row-worker thresholds already failed to improve small inserts, so repeating that tuning is not supported by the current evidence.

Temporary instrumented source and source/binary SHA256 are saved; production src/exec.rs restored byte-for-byte to HEAD BEFORE running the pinned diagnostic binary. Raw trace logs and benchmark observations retained. No production edits remain. The full nine-engine goal remains incomplete, including SQLite small/durable write losses and missing MSSQL/Kusto endpoint evidence.


</details>

### 113. 2026-10-04-current-peer-read-matrix

Артефакты: [2026-10-04-current-peer-read-matrix](../benches/results/2026-10-04-current-peer-read-matrix).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/report.md).

# Current peer read API matrix

Source: main 588136e. Pinned current release peer_bench worker; binary SHA256 in verification.json. Three independent processes per size, eight samples per case/process, at 1000/10000/100000 documents. Six matched read APIs: point get, equality count, substring count, materialize, inner join and filtered inner join. Exact values/columns/multiplicity are validated outside timing. All 42 observations in each child contain eight samples.

| Documents | Available aggregate comparisons | Lin aggregate wins | Missing peers |
|---|---:|---:|---|
| 1000 | 36 | 36 | MSSQL, Kusto |
| 10000 | 36 | 36 | MSSQL, Kusto |
| 100000 | 36 | 36 | MSSQL, Kusto |

All 108 available aggregate comparisons favor Lin. Aggregation takes a median within each process first, then a median of three process medians; this does not say Lin wins every individual process/sample. Raw batch averages are not operation-latency percentiles. The full gate remains INCOMPLETE (exit 2 for each dataset), solely due to absent MSSQL/Kusto configuration. Missing peers are never counted as wins.

| Peer | Minimum aggregate peer/Lin ratio across all 18 cases |
|---|---:|
| sqlite | 2.86x |
| duckdb | 1.16x |
| postgres | 2.61x |
| mysql | 7.31x |
| mongo | 5.45x |
| pandas | 1.46x |

This is a comparison of the implemented API paths, not intrinsic engine CPU or universal workload superiority. Lin uses prepared Rust execution and excludes JSON IPC; Python peers include their DBAPI/driver/DataFrame operations, server round trips and result materialization. Schema, seeding/preparation are outside timing. Engine order rotates across independent processes; case order remains fixed within each engine. Host load, CPU clocks and server scheduling are uncontrolled. The closest DuckDB/pandas cases have less margin than the older 2026-10-03 matrix; no causal claim about source changes explains that shift.

Dedicated ARM64 PostgreSQL 16, MySQL 8.4 and MongoDB 8.0 Docker instances use tmpfs, unique owned linbench_ fixtures and the existing clients. Actual runtime engine/client versions are in each dataset run.json. The tmpfs server results do not prove physical disk durability. The sentinel table/database rows stayed exactly unchanged after each dataset, and no fixture objects remained. Created sentinels were dropped in finally; runner exited 0 after cleanup and all three owned containers were verified stopped. final-state.json records architecture/stopped state and source-hash revalidation; verification.json records per-dataset sentinel and fixture checks.

No builds/tests overlapped timing. No production source changed. Build log, pinned binary/source hashes, exact runner and summarizer, raw process outputs and runtime versions are retained here.

The full nine-engine objective is still incomplete. Current native 1k evidence from the immediately preceding insertion comparison uses this same production baseline: Lin 1.1601665 ms vs SQLite 1.11507325 ms, 0/6 same-process wins. Durable 1k still loses in the retained sparse-classifier evidence. Local durable 10k has a small confirmed lead over SQLite, while full write comparisons across all peers and MSSQL/Kusto measurements remain unproven. Read results do not erase these gaps or redefine completion around reads alone.


</details>

<details>
<summary>rows-1000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-1/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.391 | 1.00× | validated |
| point_get | sqlite | 1.332 | 3.40× | validated |
| point_get | duckdb | 45.556 | 116.43× | validated |
| point_get | postgres | 302.243 | 772.47× | validated |
| point_get | mysql | 361.570 | 924.09× | validated |
| point_get | mongo | 486.807 | 1244.17× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.517 | 16.66× | validated |
| filter_eq | lin | 0.969 | 1.00× | validated |
| filter_eq | sqlite | 10.550 | 10.88× | validated |
| filter_eq | duckdb | 172.333 | 177.78× | validated |
| filter_eq | postgres | 352.198 | 363.34× | validated |
| filter_eq | mysql | 399.786 | 412.43× | validated |
| filter_eq | mongo | 563.192 | 581.01× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.300 | 82.84× | validated |
| text_substr | lin | 3.006 | 1.00× | validated |
| text_substr | sqlite | 44.924 | 14.94× | validated |
| text_substr | duckdb | 80.451 | 26.76× | validated |
| text_substr | postgres | 385.495 | 128.22× | validated |
| text_substr | mysql | 464.345 | 154.45× | validated |
| text_substr | mongo | 878.312 | 292.14× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 108.748 | 36.17× | validated |
| materialize | lin | 57.797 | 1.00× | validated |
| materialize | sqlite | 234.039 | 4.05× | validated |
| materialize | duckdb | 417.441 | 7.22× | validated |
| materialize | postgres | 505.781 | 8.75× | validated |
| materialize | mysql | 1586.250 | 27.44× | validated |
| materialize | mongo | 1518.472 | 26.27× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 546.930 | 9.46× | validated |
| join_inner | lin | 177.385 | 1.00× | validated |
| join_inner | sqlite | 547.576 | 3.09× | validated |
| join_inner | duckdb | 502.206 | 2.83× | validated |
| join_inner | postgres | 789.892 | 4.45× | validated |
| join_inner | mysql | 2983.521 | 16.82× | validated |
| join_inner | mongo | 18301.292 | 103.17× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 992.458 | 5.59× | validated |
| join_filter | lin | 96.071 | 1.00× | validated |
| join_filter | sqlite | 281.086 | 2.93× | validated |
| join_filter | duckdb | 330.298 | 3.44× | validated |
| join_filter | postgres | 609.573 | 6.35× | validated |
| join_filter | mysql | 1755.156 | 18.27× | validated |
| join_filter | mongo | 9716.979 | 101.14× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 909.629 | 9.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-2/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.311 | 2.13× | validated |
| point_get | duckdb | 50.659 | 82.37× | validated |
| point_get | postgres | 404.178 | 657.18× | validated |
| point_get | mysql | 997.583 | 1622.03× | validated |
| point_get | mongo | 645.029 | 1048.79× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.372 | 10.36× | validated |
| point_get | lin | 0.615 | 1.00× | validated |
| filter_eq | sqlite | 10.678 | 6.74× | validated |
| filter_eq | duckdb | 186.751 | 117.83× | validated |
| filter_eq | postgres | 436.203 | 275.23× | validated |
| filter_eq | mysql | 815.075 | 514.28× | validated |
| filter_eq | mongo | 685.619 | 432.60× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 93.113 | 58.75× | validated |
| filter_eq | lin | 1.585 | 1.00× | validated |
| text_substr | sqlite | 44.849 | 10.62× | validated |
| text_substr | duckdb | 84.870 | 20.09× | validated |
| text_substr | postgres | 466.361 | 110.38× | validated |
| text_substr | mysql | 881.046 | 208.54× | validated |
| text_substr | mongo | 904.604 | 214.11× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 138.622 | 32.81× | validated |
| text_substr | lin | 4.225 | 1.00× | validated |
| materialize | sqlite | 237.482 | 2.52× | validated |
| materialize | duckdb | 401.016 | 4.26× | validated |
| materialize | postgres | 594.824 | 6.32× | validated |
| materialize | mysql | 2844.875 | 30.22× | validated |
| materialize | mongo | 1679.511 | 17.84× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 709.057 | 7.53× | validated |
| materialize | lin | 94.135 | 1.00× | validated |
| join_inner | sqlite | 550.146 | 2.24× | validated |
| join_inner | duckdb | 484.254 | 1.97× | validated |
| join_inner | postgres | 1339.271 | 5.46× | validated |
| join_inner | mysql | 5524.021 | 22.50× | validated |
| join_inner | mongo | 19176.020 | 78.11× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1536.695 | 6.26× | validated |
| join_inner | lin | 245.506 | 1.00× | validated |
| join_filter | sqlite | 282.299 | 2.30× | validated |
| join_filter | duckdb | 330.324 | 2.69× | validated |
| join_filter | postgres | 1236.930 | 10.06× | validated |
| join_filter | mysql | 3918.188 | 31.87× | validated |
| join_filter | mongo | 10580.938 | 86.07× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 1316.375 | 10.71× | validated |
| join_filter | lin | 122.940 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-3/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 46.135 | 117.35× | validated |
| point_get | postgres | 314.640 | 800.36× | validated |
| point_get | mysql | 360.161 | 916.15× | validated |
| point_get | mongo | 483.727 | 1230.47× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.174 | 15.70× | validated |
| point_get | lin | 0.393 | 1.00× | validated |
| point_get | sqlite | 1.308 | 3.33× | validated |
| filter_eq | duckdb | 173.151 | 181.08× | validated |
| filter_eq | postgres | 352.616 | 368.75× | validated |
| filter_eq | mysql | 403.693 | 422.17× | validated |
| filter_eq | mongo | 583.740 | 610.45× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.176 | 83.84× | validated |
| filter_eq | lin | 0.956 | 1.00× | validated |
| filter_eq | sqlite | 10.690 | 11.18× | validated |
| text_substr | duckdb | 76.722 | 26.66× | validated |
| text_substr | postgres | 376.133 | 130.68× | validated |
| text_substr | mysql | 489.260 | 169.99× | validated |
| text_substr | mongo | 967.621 | 336.19× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 106.803 | 37.11× | validated |
| text_substr | lin | 2.878 | 1.00× | validated |
| text_substr | sqlite | 47.301 | 16.43× | validated |
| materialize | duckdb | 398.965 | 6.86× | validated |
| materialize | postgres | 503.893 | 8.66× | validated |
| materialize | mysql | 1725.761 | 29.65× | validated |
| materialize | mongo | 1508.785 | 25.92× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 526.822 | 9.05× | validated |
| materialize | lin | 58.199 | 1.00× | validated |
| materialize | sqlite | 236.935 | 4.07× | validated |
| join_inner | duckdb | 502.391 | 2.83× | validated |
| join_inner | postgres | 793.167 | 4.47× | validated |
| join_inner | mysql | 3131.729 | 17.66× | validated |
| join_inner | mongo | 17973.334 | 101.37× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1056.078 | 5.96× | validated |
| join_inner | lin | 177.297 | 1.00× | validated |
| join_inner | sqlite | 549.435 | 3.10× | validated |
| join_filter | duckdb | 334.313 | 3.72× | validated |
| join_filter | postgres | 585.112 | 6.52× | validated |
| join_filter | mysql | 1776.958 | 19.79× | validated |
| join_filter | mongo | 10445.624 | 116.35× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 871.729 | 9.71× | validated |
| join_filter | lin | 89.776 | 1.00× | validated |
| join_filter | sqlite | 287.581 | 3.20× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-1000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/report.md).

# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.393 | 1.00× | validated |
| point_get | sqlite | 1.311 | 3.33× | validated |
| point_get | duckdb | 46.135 | 117.35× | validated |
| point_get | postgres | 314.640 | 800.36× | validated |
| point_get | mysql | 361.570 | 919.73× | validated |
| point_get | mongo | 486.807 | 1238.30× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.372 | 16.21× | validated |
| filter_eq | lin | 0.969 | 1.00× | validated |
| filter_eq | sqlite | 10.678 | 11.02× | validated |
| filter_eq | duckdb | 173.151 | 178.63× | validated |
| filter_eq | postgres | 352.616 | 363.77× | validated |
| filter_eq | mysql | 403.693 | 416.46× | validated |
| filter_eq | mongo | 583.740 | 602.20× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.300 | 82.84× | validated |
| text_substr | lin | 3.006 | 1.00× | validated |
| text_substr | sqlite | 44.924 | 14.94× | validated |
| text_substr | duckdb | 80.451 | 26.76× | validated |
| text_substr | postgres | 385.495 | 128.22× | validated |
| text_substr | mysql | 489.260 | 162.74× | validated |
| text_substr | mongo | 904.604 | 300.89× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 108.748 | 36.17× | validated |
| materialize | lin | 58.199 | 1.00× | validated |
| materialize | sqlite | 236.935 | 4.07× | validated |
| materialize | duckdb | 401.016 | 6.89× | validated |
| materialize | postgres | 505.781 | 8.69× | validated |
| materialize | mysql | 1725.761 | 29.65× | validated |
| materialize | mongo | 1518.472 | 26.09× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 546.930 | 9.40× | validated |
| join_inner | lin | 177.385 | 1.00× | validated |
| join_inner | sqlite | 549.435 | 3.10× | validated |
| join_inner | duckdb | 502.206 | 2.83× | validated |
| join_inner | postgres | 793.167 | 4.47× | validated |
| join_inner | mysql | 3131.729 | 17.66× | validated |
| join_inner | mongo | 18301.292 | 103.17× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1056.078 | 5.95× | validated |
| join_filter | lin | 96.071 | 1.00× | validated |
| join_filter | sqlite | 282.299 | 2.94× | validated |
| join_filter | duckdb | 330.324 | 3.44× | validated |
| join_filter | postgres | 609.573 | 6.35× | validated |
| join_filter | mysql | 1776.958 | 18.50× | validated |
| join_filter | mongo | 10445.624 | 108.73× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 909.629 | 9.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-1/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.370 | 1.00× | validated |
| point_get | sqlite | 1.326 | 3.58× | validated |
| point_get | duckdb | 46.442 | 125.39× | validated |
| point_get | postgres | 261.900 | 707.09× | validated |
| point_get | mysql | 365.359 | 986.42× | validated |
| point_get | mongo | 490.188 | 1323.44× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 10.155 | 27.42× | validated |
| filter_eq | lin | 0.960 | 1.00× | validated |
| filter_eq | sqlite | 96.780 | 100.81× | validated |
| filter_eq | duckdb | 249.854 | 260.26× | validated |
| filter_eq | postgres | 547.965 | 570.78× | validated |
| filter_eq | mysql | 785.024 | 817.71× | validated |
| filter_eq | mongo | 1000.734 | 1042.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 623.838 | 649.81× | validated |
| text_substr | lin | 34.229 | 1.00× | validated |
| text_substr | sqlite | 446.540 | 13.05× | validated |
| text_substr | duckdb | 130.537 | 3.81× | validated |
| text_substr | postgres | 919.196 | 26.85× | validated |
| text_substr | mysql | 1645.195 | 48.06× | validated |
| text_substr | mongo | 3932.854 | 114.90× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1130.964 | 33.04× | validated |
| materialize | lin | 558.659 | 1.00× | validated |
| materialize | sqlite | 2564.771 | 4.59× | validated |
| materialize | duckdb | 1779.438 | 3.19× | validated |
| materialize | postgres | 2467.208 | 4.42× | validated |
| materialize | mysql | 13710.104 | 24.54× | validated |
| materialize | mongo | 7559.333 | 13.53× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 4750.855 | 8.50× | validated |
| join_inner | lin | 1820.938 | 1.00× | validated |
| join_inner | sqlite | 6365.958 | 3.50× | validated |
| join_inner | duckdb | 3689.729 | 2.03× | validated |
| join_inner | postgres | 6090.938 | 3.34× | validated |
| join_inner | mysql | 29206.625 | 16.04× | validated |
| join_inner | mongo | 174507.084 | 95.83× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 29519.250 | 16.21× | validated |
| join_filter | lin | 898.375 | 1.00× | validated |
| join_filter | sqlite | 3278.604 | 3.65× | validated |
| join_filter | duckdb | 1993.636 | 2.22× | validated |
| join_filter | postgres | 3300.458 | 3.67× | validated |
| join_filter | mysql | 16405.750 | 18.26× | validated |
| join_filter | mongo | 304562.104 | 339.01× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 9028.396 | 10.05× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-2/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 5.569 | 7.94× | validated |
| point_get | duckdb | 127.028 | 181.22× | validated |
| point_get | postgres | 583.919 | 833.04× | validated |
| point_get | mysql | 842.422 | 1201.83× | validated |
| point_get | mongo | 3957.541 | 5645.97× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 12.188 | 17.39× | validated |
| point_get | lin | 0.701 | 1.00× | validated |
| filter_eq | sqlite | 233.104 | 155.08× | validated |
| filter_eq | duckdb | 464.409 | 308.96× | validated |
| filter_eq | postgres | 1186.224 | 789.16× | validated |
| filter_eq | mysql | 1798.833 | 1196.71× | validated |
| filter_eq | mongo | 11081.916 | 7372.50× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 821.092 | 546.25× | validated |
| filter_eq | lin | 1.503 | 1.00× | validated |
| text_substr | sqlite | 1834.892 | 34.57× | validated |
| text_substr | duckdb | 275.675 | 5.19× | validated |
| text_substr | postgres | 1842.532 | 34.72× | validated |
| text_substr | mysql | 3565.041 | 67.18× | validated |
| text_substr | mongo | 11553.604 | 217.70× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 2054.257 | 38.71× | validated |
| text_substr | lin | 53.070 | 1.00× | validated |
| materialize | sqlite | 4777.000 | 2.93× | validated |
| materialize | duckdb | 3043.771 | 1.87× | validated |
| materialize | postgres | 4654.562 | 2.85× | validated |
| materialize | mysql | 27455.521 | 16.84× | validated |
| materialize | mongo | 22651.084 | 13.89× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5220.875 | 3.20× | validated |
| materialize | lin | 1630.729 | 1.00× | validated |
| join_inner | sqlite | 27824.750 | 4.18× | validated |
| join_inner | duckdb | 13071.229 | 1.96× | validated |
| join_inner | postgres | 11482.374 | 1.73× | validated |
| join_inner | mysql | 80842.438 | 12.15× | validated |
| join_inner | mongo | 332934.354 | 50.04× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 13230.251 | 1.99× | validated |
| join_inner | lin | 6653.250 | 1.00× | validated |
| join_filter | sqlite | 9193.751 | 4.85× | validated |
| join_filter | duckdb | 6086.270 | 3.21× | validated |
| join_filter | postgres | 7695.500 | 4.06× | validated |
| join_filter | mysql | 54206.626 | 28.62× | validated |
| join_filter | mongo | 240647.750 | 127.06× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7841.646 | 4.14× | validated |
| join_filter | lin | 1894.031 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-3/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 130.009 | 175.54× | validated |
| point_get | postgres | 945.649 | 1276.81× | validated |
| point_get | mysql | 1207.786 | 1630.75× | validated |
| point_get | mongo | 953.879 | 1287.92× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 14.190 | 19.16× | validated |
| point_get | lin | 0.741 | 1.00× | validated |
| point_get | sqlite | 2.128 | 2.87× | validated |
| filter_eq | duckdb | 502.940 | 279.55× | validated |
| filter_eq | postgres | 1468.208 | 816.07× | validated |
| filter_eq | mysql | 1661.750 | 923.64× | validated |
| filter_eq | mongo | 1830.344 | 1017.35× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 689.196 | 383.07× | validated |
| filter_eq | lin | 1.799 | 1.00× | validated |
| filter_eq | sqlite | 218.172 | 121.27× | validated |
| text_substr | duckdb | 283.318 | 5.03× | validated |
| text_substr | postgres | 2102.041 | 37.29× | validated |
| text_substr | mysql | 2937.896 | 52.11× | validated |
| text_substr | mongo | 6574.896 | 116.63× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1550.722 | 27.51× | validated |
| text_substr | lin | 56.374 | 1.00× | validated |
| text_substr | sqlite | 737.746 | 13.09× | validated |
| materialize | duckdb | 3101.104 | 1.97× | validated |
| materialize | postgres | 5167.187 | 3.28× | validated |
| materialize | mysql | 22031.604 | 13.99× | validated |
| materialize | mongo | 11795.396 | 7.49× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5463.105 | 3.47× | validated |
| materialize | lin | 1574.296 | 1.00× | validated |
| materialize | sqlite | 5478.562 | 3.48× | validated |
| join_inner | duckdb | 7233.105 | 1.76× | validated |
| join_inner | postgres | 12249.812 | 2.97× | validated |
| join_inner | mysql | 47338.062 | 11.49× | validated |
| join_inner | mongo | 273561.230 | 66.42× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 10728.312 | 2.60× | validated |
| join_inner | lin | 4118.646 | 1.00× | validated |
| join_inner | sqlite | 13139.104 | 3.19× | validated |
| join_filter | duckdb | 3919.896 | 2.48× | validated |
| join_filter | postgres | 7644.521 | 4.83× | validated |
| join_filter | mysql | 26635.438 | 16.83× | validated |
| join_filter | mongo | 153498.541 | 96.97× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7306.062 | 4.62× | validated |
| join_filter | lin | 1582.875 | 1.00× | validated |
| join_filter | sqlite | 6576.562 | 4.15× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-10000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/report.md).

# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.701 | 1.00× | validated |
| point_get | sqlite | 2.128 | 3.04× | validated |
| point_get | duckdb | 127.028 | 181.22× | validated |
| point_get | postgres | 583.919 | 833.04× | validated |
| point_get | mysql | 842.422 | 1201.83× | validated |
| point_get | mongo | 953.879 | 1360.84× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 12.188 | 17.39× | validated |
| filter_eq | lin | 1.503 | 1.00× | validated |
| filter_eq | sqlite | 218.172 | 145.14× | validated |
| filter_eq | duckdb | 464.409 | 308.96× | validated |
| filter_eq | postgres | 1186.224 | 789.16× | validated |
| filter_eq | mysql | 1661.750 | 1105.52× | validated |
| filter_eq | mongo | 1830.344 | 1217.68× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 689.196 | 458.50× | validated |
| text_substr | lin | 53.070 | 1.00× | validated |
| text_substr | sqlite | 737.746 | 13.90× | validated |
| text_substr | duckdb | 275.675 | 5.19× | validated |
| text_substr | postgres | 1842.532 | 34.72× | validated |
| text_substr | mysql | 2937.896 | 55.36× | validated |
| text_substr | mongo | 6574.896 | 123.89× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1550.722 | 29.22× | validated |
| materialize | lin | 1574.296 | 1.00× | validated |
| materialize | sqlite | 4777.000 | 3.03× | validated |
| materialize | duckdb | 3043.771 | 1.93× | validated |
| materialize | postgres | 4654.562 | 2.96× | validated |
| materialize | mysql | 22031.604 | 13.99× | validated |
| materialize | mongo | 11795.396 | 7.49× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5220.875 | 3.32× | validated |
| join_inner | lin | 4118.646 | 1.00× | validated |
| join_inner | sqlite | 13139.104 | 3.19× | validated |
| join_inner | duckdb | 7233.105 | 1.76× | validated |
| join_inner | postgres | 11482.374 | 2.79× | validated |
| join_inner | mysql | 47338.062 | 11.49× | validated |
| join_inner | mongo | 273561.230 | 66.42× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 13230.251 | 3.21× | validated |
| join_filter | lin | 1582.875 | 1.00× | validated |
| join_filter | sqlite | 6576.562 | 4.15× | validated |
| join_filter | duckdb | 3919.896 | 2.48× | validated |
| join_filter | postgres | 7644.521 | 4.83× | validated |
| join_filter | mysql | 26635.438 | 16.83× | validated |
| join_filter | mongo | 240647.750 | 152.03× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7841.646 | 4.95× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-1/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-1/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.654 | 1.00× | validated |
| point_get | sqlite | 2.753 | 4.21× | validated |
| point_get | duckdb | 50.562 | 77.29× | validated |
| point_get | postgres | 815.600 | 1246.79× | validated |
| point_get | mysql | 508.266 | 776.97× | validated |
| point_get | mongo | 559.562 | 855.39× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.308 | 9.64× | validated |
| filter_eq | lin | 1.729 | 1.00× | validated |
| filter_eq | sqlite | 1824.792 | 1055.25× | validated |
| filter_eq | duckdb | 604.941 | 349.83× | validated |
| filter_eq | postgres | 6108.604 | 3532.51× | validated |
| filter_eq | mysql | 5920.125 | 3423.52× | validated |
| filter_eq | mongo | 5855.542 | 3386.17× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3249.542 | 1879.16× | validated |
| text_substr | lin | 594.308 | 1.00× | validated |
| text_substr | sqlite | 8011.771 | 13.48× | validated |
| text_substr | duckdb | 685.768 | 1.15× | validated |
| text_substr | postgres | 10056.187 | 16.92× | validated |
| text_substr | mysql | 15141.500 | 25.48× | validated |
| text_substr | mongo | 34080.626 | 57.35× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6580.083 | 11.07× | validated |
| materialize | lin | 11101.500 | 1.00× | validated |
| materialize | sqlite | 49893.041 | 4.49× | validated |
| materialize | duckdb | 17471.875 | 1.57× | validated |
| materialize | postgres | 38834.938 | 3.50× | validated |
| materialize | mysql | 126736.270 | 11.42× | validated |
| materialize | mongo | 60492.562 | 5.45× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24110.438 | 2.17× | validated |
| join_inner | lin | 32201.437 | 1.00× | validated |
| join_inner | sqlite | 112474.667 | 3.49× | validated |
| join_inner | duckdb | 40759.730 | 1.27× | validated |
| join_inner | postgres | 89927.417 | 2.79× | validated |
| join_inner | mysql | 281144.834 | 8.73× | validated |
| join_inner | mongo | 1604608.021 | 49.83× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 50187.376 | 1.56× | validated |
| join_filter | lin | 17884.292 | 1.00× | validated |
| join_filter | sqlite | 51069.104 | 2.86× | validated |
| join_filter | duckdb | 20684.438 | 1.16× | validated |
| join_filter | postgres | 46608.792 | 2.61× | validated |
| join_filter | mysql | 130725.958 | 7.31× | validated |
| join_filter | mongo | 800984.397 | 44.79× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 26087.541 | 1.46× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-2/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-2/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.357 | 3.69× | validated |
| point_get | duckdb | 48.574 | 132.15× | validated |
| point_get | postgres | 327.365 | 890.62× | validated |
| point_get | mysql | 423.639 | 1152.54× | validated |
| point_get | mongo | 554.390 | 1508.25× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.277 | 17.08× | validated |
| point_get | lin | 0.368 | 1.00× | validated |
| filter_eq | sqlite | 962.304 | 1004.82× | validated |
| filter_eq | duckdb | 584.706 | 610.54× | validated |
| filter_eq | postgres | 3246.292 | 3389.70× | validated |
| filter_eq | mysql | 4613.479 | 4817.29× | validated |
| filter_eq | mongo | 5412.083 | 5651.17× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3151.688 | 3290.92× | validated |
| filter_eq | lin | 0.958 | 1.00× | validated |
| text_substr | sqlite | 4856.417 | 17.03× | validated |
| text_substr | duckdb | 633.938 | 2.22× | validated |
| text_substr | postgres | 7115.333 | 24.95× | validated |
| text_substr | mysql | 13164.292 | 46.16× | validated |
| text_substr | mongo | 32691.688 | 114.63× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6369.270 | 22.33× | validated |
| text_substr | lin | 285.195 | 1.00× | validated |
| materialize | sqlite | 29434.209 | 4.30× | validated |
| materialize | duckdb | 16849.250 | 2.46× | validated |
| materialize | postgres | 23401.791 | 3.42× | validated |
| materialize | mysql | 118855.792 | 17.35× | validated |
| materialize | mongo | 59779.291 | 8.73× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24208.084 | 3.53× | validated |
| materialize | lin | 6849.521 | 1.00× | validated |
| join_inner | sqlite | 72041.563 | 3.86× | validated |
| join_inner | duckdb | 40100.812 | 2.15× | validated |
| join_inner | postgres | 61243.896 | 3.28× | validated |
| join_inner | mysql | 236651.187 | 12.68× | validated |
| join_inner | mongo | 1584562.354 | 84.90× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 49905.979 | 2.67× | validated |
| join_inner | lin | 18663.791 | 1.00× | validated |
| join_filter | sqlite | 35326.438 | 3.83× | validated |
| join_filter | duckdb | 19921.729 | 2.16× | validated |
| join_filter | postgres | 32633.562 | 3.54× | validated |
| join_filter | mysql | 127174.042 | 13.79× | validated |
| join_filter | mongo | 826456.833 | 89.65× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 25972.791 | 2.82× | validated |
| join_filter | lin | 9218.959 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/process-3/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-3/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 100.221 | 153.11× | validated |
| point_get | postgres | 572.190 | 874.17× | validated |
| point_get | mysql | 585.573 | 894.62× | validated |
| point_get | mongo | 1065.437 | 1627.74× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 9.965 | 15.22× | validated |
| point_get | lin | 0.655 | 1.00× | validated |
| point_get | sqlite | 1.952 | 2.98× | validated |
| filter_eq | duckdb | 948.758 | 562.12× | validated |
| filter_eq | postgres | 4098.896 | 2428.51× | validated |
| filter_eq | mysql | 5311.062 | 3146.70× | validated |
| filter_eq | mongo | 7122.687 | 4220.05× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 4577.375 | 2712.00× | validated |
| filter_eq | lin | 1.688 | 1.00× | validated |
| filter_eq | sqlite | 1297.646 | 768.83× | validated |
| text_substr | duckdb | 906.179 | 1.80× | validated |
| text_substr | postgres | 9152.938 | 18.22× | validated |
| text_substr | mysql | 16170.271 | 32.19× | validated |
| text_substr | mongo | 39759.896 | 79.14× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 8068.105 | 16.06× | validated |
| text_substr | lin | 502.368 | 1.00× | validated |
| text_substr | sqlite | 8501.458 | 16.92× | validated |
| materialize | duckdb | 23660.188 | 1.81× | validated |
| materialize | postgres | 31799.542 | 2.44× | validated |
| materialize | mysql | 161253.250 | 12.36× | validated |
| materialize | mongo | 76231.667 | 5.84× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 42878.605 | 3.29× | validated |
| materialize | lin | 13045.188 | 1.00× | validated |
| materialize | sqlite | 47016.959 | 3.60× | validated |
| join_inner | duckdb | 58360.417 | 2.21× | validated |
| join_inner | postgres | 96616.875 | 3.65× | validated |
| join_inner | mysql | 380077.605 | 14.37× | validated |
| join_inner | mongo | 1690539.604 | 63.94× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 65401.374 | 2.47× | validated |
| join_inner | lin | 26440.583 | 1.00× | validated |
| join_inner | sqlite | 154655.541 | 5.85× | validated |
| join_filter | duckdb | 21802.833 | 1.14× | validated |
| join_filter | postgres | 59599.813 | 3.12× | validated |
| join_filter | mysql | 169944.166 | 8.90× | validated |
| join_filter | mongo | 1089726.062 | 57.10× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 44331.854 | 2.32× | validated |
| join_filter | lin | 19085.105 | 1.00× | validated |
| join_filter | sqlite | 65138.145 | 3.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

<details>
<summary>rows-100000/report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/report.md).

# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.654 | 1.00× | validated |
| point_get | sqlite | 1.952 | 2.98× | validated |
| point_get | duckdb | 50.562 | 77.29× | validated |
| point_get | postgres | 572.190 | 874.69× | validated |
| point_get | mysql | 508.266 | 776.97× | validated |
| point_get | mongo | 559.562 | 855.39× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.308 | 9.64× | validated |
| filter_eq | lin | 1.688 | 1.00× | validated |
| filter_eq | sqlite | 1297.646 | 768.83× | validated |
| filter_eq | duckdb | 604.941 | 358.42× | validated |
| filter_eq | postgres | 4098.896 | 2428.51× | validated |
| filter_eq | mysql | 5311.062 | 3146.70× | validated |
| filter_eq | mongo | 5855.542 | 3469.29× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3249.542 | 1925.29× | validated |
| text_substr | lin | 502.368 | 1.00× | validated |
| text_substr | sqlite | 8011.771 | 15.95× | validated |
| text_substr | duckdb | 685.768 | 1.37× | validated |
| text_substr | postgres | 9152.938 | 18.22× | validated |
| text_substr | mysql | 15141.500 | 30.14× | validated |
| text_substr | mongo | 34080.626 | 67.84× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6580.083 | 13.10× | validated |
| materialize | lin | 11101.500 | 1.00× | validated |
| materialize | sqlite | 47016.959 | 4.24× | validated |
| materialize | duckdb | 17471.875 | 1.57× | validated |
| materialize | postgres | 31799.542 | 2.86× | validated |
| materialize | mysql | 126736.270 | 11.42× | validated |
| materialize | mongo | 60492.562 | 5.45× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24208.084 | 2.18× | validated |
| join_inner | lin | 26440.583 | 1.00× | validated |
| join_inner | sqlite | 112474.667 | 4.25× | validated |
| join_inner | duckdb | 40759.730 | 1.54× | validated |
| join_inner | postgres | 89927.417 | 3.40× | validated |
| join_inner | mysql | 281144.834 | 10.63× | validated |
| join_inner | mongo | 1604608.021 | 60.69× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 50187.376 | 1.90× | validated |
| join_filter | lin | 17884.292 | 1.00× | validated |
| join_filter | sqlite | 51069.104 | 2.86× | validated |
| join_filter | duckdb | 20684.438 | 1.16× | validated |
| join_filter | postgres | 46608.792 | 2.61× | validated |
| join_filter | mysql | 130725.958 | 7.31× | validated |
| join_filter | mongo | 826456.833 | 46.21× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 26087.541 | 1.46× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.


</details>

### 114. 2026-10-04-durable-phase-refresh

Артефакты: [2026-10-04-durable-phase-refresh](../benches/results/2026-10-04-durable-phase-refresh).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-durable-phase-refresh/report.md).

# Current durable insert phase diagnostic

Source: main c21d893 (includes retained row-major large-batch packing and current ASCII/default-bigram embedding optimizations). Two separately pinned diagnostic binaries: initial phase breakdown and independent finer per-column breakdown. Production src/exec.rs and src/persist.rs were restored byte-for-byte before either binary was run. Source snapshots and hashes are retained; no instrumentation is present in main.

Each binary ran three independent processes, each selecting existing compare/durable_insert_*/lin with 24 fresh samples, one operation per sample, no warmup. Existing checked fixture validation verifies insertion count and full expected row values outside timing. All six runs completed successfully. Logs contain 27 phase events per size per process, including validation/calibration operations in addition to measured samples; every reported phase has the same event count. Summaries use the median of events in each process, then median of the three process medians. These diagnostics are not peer benchmarks and cannot establish the named-peer objective.

## Initial phase breakdown

Milliseconds:

| Phase | 1k | 10k |
|---|---:|---:|
| build | 0.284333 | 1.896375 |
| checks | 0.143250 | 1.520667 |
| embed | 0.370625 | 4.017667 |
| index | 0.125625 | 1.192750 |
| fts | 0.183709 | 1.816583 |
| maps | 0.036000 | 0.452625 |
| pack | 0.228792 | 2.244958 |
| classify | 0.123916 | 1.246125 |
| encode | 0.672250 | 6.879666 |
| checksum | 0.015500 | 0.157291 |
| write | 0.045167 | 0.286334 |
| sync | 0.872417 | 1.428666 |

build is pure Row construction (including worker creation/join for the existing large-batch path); checks includes default IDs/hashes, uniqueness and FK checks; embed includes text preparation and vector assignment; index/fts/maps measure their existing slab operations. pack measures the durable rows_to_insert_cols call. encode measures column payload encoding, including sparse classification; checksum measures CRC/envelope preparation; write measures the existing four write_all calls; sync measures existing Full durable_sync.

Sparse classification is nested inside encode: do not add them. Phase medians must not be summed to estimate total duration or subtracted as an exact decomposition. Diagnostic Instant calls and stderr logging perturb the path; outer encode includes the classify diagnostic logging cost. Setup/checkpoint frames are excluded from per-insert frame statistics by pairing each WAL_ENCODE event with its next WAL_FRAME and verifying matching payload byte length. Other insert phases omit moving rows into the store and unrelated call overhead.

## Independent per-column breakdown

Milliseconds, measured by the second diagnostic binary:

| Column | 1k | 10k |
|---|---:|---:|
| body | 0.003417 | 0.053416 |
| embedding | 0.647750 | 6.609250 |
| hash | 0.004916 | 0.051459 |
| id | 0.003875 | 0.095458 |
| layer | 0.003417 | 0.055709 |
| title | 0.004250 | 0.044833 |
| ts | 0.000917 | 0.007417 |
| uri | 0.003375 | 0.037125 |
| wing | 0.002917 | 0.028917 |

Second-series aggregate encode medians: 0.777333 ms / 7.114083 ms; sparse classification: 0.124083 ms / 1.286917 ms. Column embedding includes sparse classification and its diagnostic logging cost. All nine per-column timers include field-name serialization. Extra per-column stderr logging lies within the outer encode timer, so comparing the two diagnostic series as a performance improvement would be invalid.

Payload bytes are constant within each size and across both series: 336968 at 1k and 3558340 at 10k. Normal float bits, negative zero/NaN handling and persisted codec behavior were not changed by these diagnostics.

The evidence points toward vector encoding as the next optimization target, rather than further text-column or syscall tuning. In the fine-grained diagnostic the embedding column records roughly 6.61 ms at 10k, versus at most 0.10 ms for any text column. A next experiment can divide sparse-vector encoding across a bounded number of workers for large batches, preserving exact row order, float bits, encoded bytes, decoded-budget validation, errors and durable-sync timing. Additional CPU/temporary buffers would need to be reported and benchmarked. This hypothesis is not implemented or proven here.

No production changes or new tests were retained, so a full workspace test rerun was not required. All changes in this commit are diagnostic evidence. Durable SQLite gaps and unverified MSSQL/Kusto benchmarks remain open.


</details>

### 115. 2026-10-04-embed-ascii-split

Артефакты: [2026-10-04-embed-ascii-split](../benches/results/2026-10-04-embed-ascii-split).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-embed-ascii-split/report.md).

# ASCII whitespace splitting in hashing embedding: retained

Baseline: 2182950. Candidate uses split_ascii_whitespace for ASCII text without vertical tab; all other text keeps split_whitespace. The vertical-tab guard preserves the original Unicode whitespace semantics. Common token feature work is extracted into a private bump_token helper. All unigram/trigram/bigram ordering, weights, normalization, model identity, arbitrary dimensions and persisted f32 bits stay unchanged. No new table, public API, wire format or durability tradeoff.

Six alternating independent baseline/candidate pairs per series; 24 fresh fixtures/process, one operation/sample, no warmup. Preparation/schema/setup excluded; embedding/thread/index/durable flush work remains timed. Exact fixture/count/value/readback gates passed. Builds/tests did not overlap timing. Repeat used the same pinned binaries without rebuilding.

Mixed-size insertion series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 13.83% | 6/6 |
| compare/insert_bulk_1k/sqlite | 3.86% | 4/6 |
| compare/insert_bulk_10k/lin | 5.30% | 6/6 |
| compare/insert_bulk_10k/sqlite | 3.23% | 4/6 |
| compare/durable_insert_1k/lin | 3.58% | 5/6 |
| compare/durable_insert_1k/sqlite | 7.34% | 5/6 |
| compare/durable_insert_10k/lin | 1.10% | 3/6 |
| compare/durable_insert_10k/sqlite | 5.25% | 5/6 |

Independent native 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 6.16% | 6/6 |
| compare/insert_bulk_1k/sqlite | -0.77% | 2/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Mixed-size native gains are partly confounded by positive SQLite controls. The independent small-native repeat favors Lin in all six pairs (+6.16%) while SQLite control is approximately flat/slower (-0.77%), supporting a repeatable improvement on that workload. Durable gains are mixed and SQLite controls improved more; no durable acceleration is established by this evidence. Larger-native results also include positive control movement and are not a universal causal claim. Host load/cache/frequency remain uncontrolled, and absolute medians differed between series; no pooling of the two series was performed.

Candidate native 1k still loses to SQLite: mixed-size aggregate medians 1.250948 vs 1.216781 ms (0/6 peer wins), repeat 1.155813 vs 1.107542 ms (1/6 wins). Native 10k favors Lin in all six mixed-size pairs. Durable 1k/10k remain slower than SQLite. The full nine-engine goal remains incomplete, including missing MSSQL/Kusto endpoint proof.

Validation: six focused embedding tests passed. The original dense-reference bit test was expanded across all 128 ASCII characters between tokens (including VT, FF, CR/LF, tabs, NUL and mixed case), Unicode whitespace U+0085/U+00A0/U+1680/U+2000/U+200A/U+2028/U+2029/U+202F/U+205F/U+3000, plus existing ASCII/Unicode/empty/repeated inputs, at dimensions 8/9/32/384/768/1024/1536. Full `cargo test --offline --workspace` passed. Both benchmark builds passed. `git diff --check` passed. final-embed.rs adds only an explanatory comment after testing/measurement; benchmark candidate source and source/binary hashes are saved separately.

Decision: retain the repeated small-native improvement with exact compatibility. These measurements do not prove Unicode-path performance, neural-backend performance, per-engine CPU superiority or physical disk portability.


</details>

### 116. 2026-10-04-embed-fixed-trigrams

Артефакты: [2026-10-04-embed-fixed-trigrams](../benches/results/2026-10-04-embed-fixed-trigrams).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-embed-fixed-trigrams/report.md).

# Fixed-size trigram hash input: rejected

Baseline: 0041eef. Candidate passes [window[0], window[1], window[2]] into Scratch::bump rather than its three-byte slice, to expose constant length through the generic Hash input type. No additional table/heap allocation, model identity or wire-format changes. Array/slice hashing preserves the observed vector bits; sources and binary hashes identify the measured change.

Six alternating independent process pairs per series, 24 fresh fixtures per process, one operation/sample and no warmup. Fixture/schema/preparation excluded; normal insert work including embedding and durability sync remains timed. Exact fixture/count/value/readback checks passed. No builds/tests overlapped timing. Independent 1k repeat used the same pinned binaries without rebuilding.

Mixed-size series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | 0.91% | 5/6 |
| compare/insert_bulk_1k/sqlite | -1.73% | 2/6 |
| compare/insert_bulk_10k/lin | -0.01% | 3/6 |
| compare/insert_bulk_10k/sqlite | 0.22% | 4/6 |
| compare/durable_insert_1k/lin | 1.97% | 4/6 |
| compare/durable_insert_1k/sqlite | -0.26% | 3/6 |
| compare/durable_insert_10k/lin | 0.40% | 4/6 |
| compare/durable_insert_10k/sqlite | 0.04% | 3/6 |

Independent native 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.23% | 2/6 |
| compare/insert_bulk_1k/sqlite | -0.26% | 2/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Initial small-native gain is weak and not replicated: the independent repeat changes Lin -0.23% and SQLite control -0.26%, both faster in only 2/6 pairs. Larger native is essentially unchanged. Durable indications are mixed and do not prove a generally useful improvement. No generated-code inspection was performed; these wall times do not establish whether compiler specialization removes checks or yields identical machine code.

Decision: reject the unconfirmed performance change. Production src/embed.rs restored byte-for-byte to baseline, retaining the separately validated ASCII whitespace optimization. Six focused embedding tests passed, including dense-reference f32 bits for all ASCII characters, Unicode whitespace, existing mixed/empty/repeated texts and dimensions 8/9/32/384/768/1024/1536. Both benchmark binaries built. Full workspace tests were not run for this rejected candidate. `git diff --check` passed after log trailing-blank normalization and source restoration.

The full nine-engine objective remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements. This is evidence against retaining this specialization, not a new speedup or peer matrix refresh.


</details>

### 117. 2026-10-04-embed-trigram-table

Артефакты: [2026-10-04-embed-trigram-table](../benches/results/2026-10-04-embed-trigram-table).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-embed-trigram-table/report.md).

# ASCII trigram embedding table: rejected

Baseline: 5a6937b. Candidate computes 26^3 ASCII lowercase-letter trigram slots using the active FxHasher/slice Hash implementation for dimension 768; other triples/dimensions use the original hash path. Feature order/weights/normalization and vector bits remain unchanged. The additional table payload is 35,152 bytes (about 34 KiB), with one-time initialization in the first default embedder constructor. Other-dimension paths also compute range indices before their fallback in this measured candidate. No model identity or wire-format change.

Seven focused embedding tests passed, including exhaustive slot checks for all 17,576 letter triples and original dense-reference f32-bit comparisons across dimensions and ASCII/Unicode/empty/repeated inputs. Both benchmark binaries built. Full workspace tests and constructor cold-start timing were not run for this rejected candidate.

Six independent alternating baseline/candidate pairs for each workload; 24 fresh fixtures per process, one operation/sample, no warmup. Schema/preparation/setup, including constructor table initialization, excluded. Exact fixture/count/value/readback gates passed; thread/embedding/index work remains in the timed insert. No compilation/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -3.53% | 1/6 |
| compare/insert_bulk_1k/sqlite | -1.03% | 2/6 |
| compare/insert_bulk_10k/lin | -4.38% | 1/6 |
| compare/insert_bulk_10k/sqlite | 1.09% | 4/6 |
| compare/durable_insert_1k/lin | 1.20% | 4/6 |
| compare/durable_insert_1k/sqlite | 1.79% | 4/6 |
| compare/durable_insert_10k/lin | 1.87% | 5/6 |
| compare/durable_insert_10k/sqlite | 1.24% | 5/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Both native Lin workloads regressed in five of six pairs; native 10k SQLite control improved, making the native regression harder to explain as common host slowdown. Durable gains partly track positive SQLite controls and do not offset the native regression. Whole-insert measurements do not separately establish the CPU cost of range checks, table loads or cache effects.

Decision: reject this representation/lookup change. Production src/embed.rs restored byte-for-byte to baseline, retaining the separately measured bigram lookup. Raw sources, binary/source hashes, logs and observations saved. `git diff --check` passed after log trailing-blank normalization and restoration. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto measurements.


</details>

### 118. 2026-10-04-four-way-sparse-wal

Артефакты: [2026-10-04-four-way-sparse-wal](../benches/results/2026-10-04-four-way-sparse-wal).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-four-way-sparse-wal/report.md).

# Four-way sparse WAL encoding

Retained on main baseline bab6848: change only append_sparse_rows_parallel to partition rows into at most four consecutive chunks rather than two halves. Create at most three scoped workers, encode the first chunk into the existing buffer and append worker buffers in original row order. Each worker-creation failure uses the existing serial fallback for that chunk. All workers are joined before WAL writing and durability flush. Small batches retain the existing serial encoder; existing 4096-row and 8MiB decoded-float gates, shared decoded-budget validation, sparse classification, wire codec/tag, exact float bits, CRC and Full sync remain unchanged.

Resource tradeoff: this encoding stage uses up to four CPU execution contexts instead of two, with up to three temporary output buffers. Roughly three quarters of encoded column bytes now pass through worker buffers/merge rather than half. There is also a small worker-handle vector and more thread creation/join. CPU energy/peak RSS were not measured. This is a latency optimization with additional parallel CPU resources, not a per-core throughput claim.

## Measured durable inserts

Two independent series on exactly the same pinned binaries, each six alternating baseline/candidate process pairs, 24 fresh checked fixtures per case, one operation per sample, no warmup. No builds/tests overlapped timing. Existing exact row validation runs outside timing. Percentages are median paired improvements from six process medians, not aggregate-median ratios. SQLite is an unchanged control. Background load and CPU frequency are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | +2.673%, 6/6 | -0.651%, 2/6 | +8.001%, 6/6 | +0.158%, 3/6 |
| Independent repeat | +6.832%, 5/6 | +4.202%, 3/6 | +8.282%, 5/6 | +4.189%, 5/6 |

Both series are positive at 10k with six/five wins. The first unchanged SQLite control is nearly flat at 10k, whereas repeat control also improves 4.189%; the entire 8.282% repeat difference cannot be attributed to this change. The small serial path also appears faster, although no workers are created there; do not attribute that difference to parallel encoding. Keep the change for the repeated positive large-case observations, with these control/resource limits explicit.

Aggregate medians of process medians, milliseconds; these are separate statistics from the paired gains:

| Series/size | Lin baseline | Lin candidate | SQLite candidate control | Lin-vs-SQLite wins |
|---|---:|---:|---:|---:|
| First series 1k | 3.133583 | 3.023062 | 1.826490 | 0/6 |
| First series 10k | 21.131218 | 19.468104 | 18.937448 | 1/6 |
| Independent repeat 1k | 3.431490 | 3.311719 | 1.978500 | 0/6 |
| Independent repeat 10k | 20.964792 | 20.429709 | 18.950969 | 2/6 |

SQLite remains faster in both aggregate comparisons. First-series 10k is close, but the repeat has a larger gap. This does not complete the named-peer goal, prove universal speedup, or establish MSSQL/Kusto results.

## Verification/provenance

All nine focused WAL integrity tests and full workspace tests passed. Existing independent scalar-reference comparison now exercises four chunks over 4101 rows, including the uneven last chunk and None/Some(empty), negative zero, NaN payload, infinity and last coordinates; actual WAL payload bytes and decoded values are checked. Worker fallback injection preserves exact bytes. The multi-column shared-64MiB budget test preserves existing WAL bytes/counter on error. No actual OS resource exhaustion was induced.

Baseline binary SHA256 matches the retained two-thread final executable metadata from ../2026-10-04-parallel-sparse-wal/final-binary-sha256.json. before-persist.rs production code is byte-identical to that measured source; later changes only added a budget test and evidence. Candidate source stayed byte-identical through both measurements and full tests. Raw results, source/binary SHA256 hashes, build/test logs and invocation scripts are retained here. The goal remains active.


</details>

### 119. 2026-10-04-fts-ascii-split

Артефакты: [2026-10-04-fts-ascii-split](../benches/results/2026-10-04-fts-ascii-split).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-fts-ascii-split/report.md).

# Rejected FTS append ASCII whitespace split

Baseline main 3478392. Candidate replaces split_whitespace in append_row_with_scratch with split_ascii_whitespace when lowered text is ASCII and contains no vertical tab. Unicode/vertical-tab text keeps the original splitter. Existing lowercase transformation, fields, token order, posting insertion/uniqueness, pending mutations, query tokenization and persisted layout remain unchanged. No allocation table, dependency or CPU worker was added.

## Paired insertion benchmarks

Six alternating baseline/candidate process pairs for native and durable fixtures, 24 fresh checked samples per case, one operation/sample, no warmup. Exact existing fixture validation runs outside timing. Pinned binaries; builds/tests did not overlap timing. SQLite is an unchanged control. Percentages are median paired improvements from per-process medians, not aggregate-median ratios. CPU frequency/background load are uncontrolled.

| Mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Native | -0.374%, 3/6 | +0.642%, 4/6 | -1.683%, 0/6 | +0.356%, 4/6 |
| Durable | +0.230%, 3/6 | +0.981%, 5/6 | -0.396%, 2/6 | +0.076%, 3/6 |

Reject: native 10k regresses in all six pairs while the unchanged SQLite control is slightly positive. Other Lin cases are mixed or slightly negative; no consistent gain is demonstrated. The embedding ASCII-split result cannot be generalized to this posting-update loop. No assembler/profile attribution is claimed and no repeat was run: the six native-10k regressions were sufficient to reject this candidate.

## Validation and restoration

Eight focused FTS tests passed. New coverage compares the complete posting map against the original Unicode whitespace splitter over all 128 ASCII symbols, ten Unicode whitespace characters, Greek/other Unicode, repeated words across title/body, empty text and vertical tab, preserving the existing lowercase helper semantics. Pending adds/dels remain empty for the fresh slab. Existing pending-edit, recycled-tail and reference tests passed. Full workspace was not rerun because production and the new test were restored byte-for-byte from before-fts.rs.

Baseline executable SHA256 matches the retained sparse-classifier candidate metadata. Source/binary hashes, raw outputs, invocation scripts and build/test logs are retained. Prior WAL optimizations and local durable-10k SQLite lead remain unchanged. Native/durable 1k and the full named-peer objective remain unresolved.


</details>

### 120. 2026-10-04-mimalloc-prototype

Артефакты: [2026-10-04-mimalloc-prototype](../benches/results/2026-10-04-mimalloc-prototype).

<details>
<summary>README.md</summary>

Источник: [README.md](../benches/results/2026-10-04-mimalloc-prototype/README.md).

# MiMalloc prototype: rejected after independent repeat

Baseline production: 2f59f1a (production code identical to retained 4926d9c). Candidate selects mimalloc 0.1.52 only in the compare benchmark root and lib unit-test root; it does not change the CLI/library runtime allocator. Default mimalloc features, no C allocator override. Dependency: https://github.com/purpleprotocol/mimalloc_rust .

Each round uses six alternating baseline/candidate process pairs per mode, 24 fresh fixtures per case, one operation per sample, no warmup. Both rounds use the same pinned release binaries (SHA-256 recorded). Native and Full durable insert cases include Lin and SQLite at 1k and 10k. Figures are median paired percent latency improvement; positive is faster. Wins count process-median pairs, not individual samples.

| Case | First gain / wins | Repeat gain / wins |
|---|---:|---:|
| Lin native 1k | +10.205% / 5 of 6 | -9.621% / 2 of 6 |
| SQLite native 1k | +1.269% / 4 of 6 | -11.413% / 2 of 6 |
| Lin native 10k | +11.358% / 4 of 6 | +3.925% / 5 of 6 |
| SQLite native 10k | -1.819% / 2 of 6 | +14.148% / 6 of 6 |
| Lin durable 1k | +4.973% / 5 of 6 | -20.290% / 2 of 6 |
| SQLite durable 1k | +5.594% / 5 of 6 | -28.750% / 1 of 6 |
| Lin durable 10k | +1.741% / 4 of 6 | +9.747% / 3 of 6 |
| SQLite durable 10k | +6.043% / 4 of 6 | +7.710% / 5 of 6 |

The smaller workloads regress in the repeat and the control shifts materially. SQLite Rust wrappers and the harness also change allocator, so SQLite is not an unchanged control in this experiment. These measurements do not isolate allocator causality, prove CLI performance, or establish superiority over all named peers. Do not retain this prototype based on the first round.

Validation: corrected lib unit-test executable passes 45 tests under MiMalloc; corrected benchmark build and both paired runs complete. Initial failed logs document a prototype placement error (allocator inserted into bench prose and between an existing GPU cfg attribute and its module); placement was repaired from original snapshots before successful validation. No full workspace run was required for this rejected candidate. All four production files, including Cargo.lock, restored byte-for-byte from before snapshots. No dependency or allocator change ships.

Root and repeat medians.json contain process medians; run directories contain observations and fixture validation results. Candidate/before snapshots preserve the rejected implementation. MSSQL and Kusto endpoints remain unconfigured; small native and durable SQLite gaps remain unresolved.


</details>

### 121. 2026-10-04-normalization-half-density

Артефакты: [2026-10-04-normalization-half-density](../benches/results/2026-10-04-normalization-half-density).

<details>
<summary>README.md</summary>

Источник: [README.md](../benches/results/2026-10-04-normalization-half-density/README.md).

# Half-density sparse normalization: rejected

Baseline production 0f811a3 (production unchanged from retained classifier 4926d9c). Candidate changes only the sparse-normalization gate from touched*5 < dimension to touched*2 < dimension, retaining existing exact-quarter-weight reasoning and ascending-order fallback at large sums. No allocator, embedding feature weights, hash, vector format, WAL, sync or worker change.

Seven focused embedding tests pass, including new bit-exact comparisons at 153, 154, 200, 383, 384, 500 and 768 touched coordinates with small and large quarter weights. Scratch reset is checked. Benchmark build completes. Six alternating process pairs for native and Full durable, 24 fresh fixtures per case, one operation per sample, no warmup. No compilation or tests during timing. Binary hashes recorded. Positive paired gain means faster; wins are process-median wins.

| Case | Median paired gain | Wins | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | +0.230% | 4/6 | 0.8853 | 0.8868 |
| compare/insert_bulk_1k/sqlite | -0.611% | 3/6 | 0.8199 | 0.8193 |
| compare/insert_bulk_10k/lin | -0.047% | 3/6 | 8.9276 | 8.9689 |
| compare/insert_bulk_10k/sqlite | -0.067% | 3/6 | 10.9160 | 10.6941 |
| compare/durable_insert_1k/lin | +0.520% | 4/6 | 3.9668 | 3.7934 |
| compare/durable_insert_1k/sqlite | -0.116% | 3/6 | 2.1714 | 2.2686 |
| compare/durable_insert_10k/lin | +2.948% | 4/6 | 23.7019 | 23.0014 |
| compare/durable_insert_10k/sqlite | -4.945% | 2/6 | 28.2893 | 28.3303 |

Small native/durable gains (0.23% and 0.52%) do not close the SQLite gap, native 10k is flat, and durable 10k is inconsistent with a material opposite control shift. No repeat or full workspace run performed: insufficient evidence to retain the gate change. Production src/embed.rs restored byte-for-byte from before snapshot; candidate and tests preserved here only. Named-peer objective remains incomplete, including small SQLite insert gaps and missing MSSQL/Kusto configuration.


</details>

### 122. 2026-10-04-parallel-sparse-classifier

Артефакты: [2026-10-04-parallel-sparse-classifier](../benches/results/2026-10-04-parallel-sparse-classifier).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-parallel-sparse-classifier/report.md).

# Parallel exact sparse-vector size classification

Retained on baseline main f1b5219: for at least 4096 rows and estimated dense bytes at least 8MiB, compute the exact sparse serialized size in at most four consecutive chunks with up to three scoped workers. Sum per-chunk nonnegative sizes with saturating arithmetic, then retain the original sparse < dense decision. Empty-vector preservation still returns true before this branch; small columns retain the original serial classification. Worker-creation failures recompute the affected chunk serially. Worker panics propagate. No per-row count cache, new format, byte encoding, default embedding or durability changes.

The new classification workers finish before the existing four-way encoder workers start. Peak parallelism remains at most four execution contexts in each stage, but a large vector column now creates up to three additional scoped workers and has another parallel stage. Classification does not clone vector payloads or keep a count table; it reads immutable slices and returns integer sizes. Small worker-handle storage and OS thread resources are additional; peak RSS/CPU energy were not measured. Gates are conservative, not optimal crossover measurements.

## Durable measurements

Two independent six-pair series using exactly the same pinned binaries; alternating baseline/candidate order, 24 fresh checked fixtures per case, one operation/sample, no warmup. No builds/tests overlapped timing. Existing full row/count validation runs outside timing. SQLite is an unchanged control. Reported percentages are medians of paired percentage improvements from per-process medians, not ratios of aggregate medians. CPU frequency/background load are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | -0.929%, 3/6 | +0.318%, 4/6 | +4.730%, 6/6 | -0.716%, 2/6 |
| Independent repeat | -0.164%, 3/6 | -0.245%, 2/6 | +3.889%, 5/6 | +0.093%, 3/6 |

Large-case gains repeat with six/five wins, while unchanged SQLite controls shift by -0.716% / +0.093%, smaller than Lin's gains. Small-case results are slightly negative and mixed (3/6 each): no small-case acceleration is claimed, and these measurements do not demonstrate an isolated causal penalty from the new guard. Both aggregate small-case differences are roughly 0.01–0.04 ms and remain an open workload to improve.

Aggregate medians of process medians, milliseconds (separate from paired gain statistics):

| Series/size | Lin baseline | Lin candidate | SQLite candidate control | Lin-vs-SQLite wins |
|---|---:|---:|---:|---:|
| First series 1k | 3.068510 | 3.107333 | 1.844011 | 0/6 |
| First series 10k | 19.729896 | 18.726000 | 18.964865 | 5/6 |
| Independent repeat 1k | 3.187906 | 3.200708 | 1.891417 | 0/6 |
| Independent repeat 10k | 19.289750 | 18.587021 | 18.872573 | 5/6 |

For the existing local 10k durable fixture, the retained candidate has a small aggregate lead over SQLite in both series and wins five of six same-process comparisons each. The margin is about 1–2%, so do not generalize to universal engine superiority, unmeasured workloads/hardware, or equal per-core cost. Lin still loses durable 1k, prior native 1k remains unresolved, and full current named-peer proof including MSSQL/Kusto remains incomplete. This is progress toward the full goal, not completion.

## Validation and provenance

All ten focused WAL integrity tests and full workspace tests passed. New coverage verifies exact size counts against fixtures with known nonzero bits, None and sparse/dense vectors; dimension-independent negative zero/NaN/infinity counting; zero/one/three-row helper boundaries; 4095/4096/4101-row selection; Some(empty) override; and injected WouldBlock worker-creation fallback. Existing independent scalar-reference WAL byte comparison, vector replay, corruption/truncation/CRC and shared decoded-budget/no-write-on-error tests passed. No actual OS resource exhaustion was induced.

before-persist.rs is byte-identical to the retained four-way candidate source; baseline binary hash matches ../2026-10-04-four-way-sparse-wal/binary-sha256.json candidate. Candidate source remained byte-identical through both timed series and full workspace tests. Raw outputs, separate source/binary hashes, environment and invocation scripts are retained here. The goal remains active.


</details>

### 123. 2026-10-04-parallel-sparse-wal

Артефакты: [2026-10-04-parallel-sparse-wal](../benches/results/2026-10-04-parallel-sparse-wal).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-parallel-sparse-wal/report.md).

# Parallel encoding of large sparse WAL vector columns

Retained final implementation on main baseline 73bd87a: when a sparse vector column has at least 4096 rows and at least 8MiB of decoded float data, validate the shared decoded budget before encoding, split rows in half, encode the left half into the existing buffer and the right half with one scoped worker into a temporary buffer, then append the right bytes. All other batches retain the original checked per-row encoder loop. A failure to create the additional worker falls back to serial encoding of the right half; a worker panic still propagates. The parallelism is bounded to two threads for this encoding stage, and the worker is joined before write/flush.

The same sparse selection, codec 3/tag 7, row order, exact float bits, presence semantics, maximum record length, CRC and Full durable_sync are preserved. Cumulative decoded memory validation applies across columns, including Option<Vec> metadata. Resource policy: the worker uses an additional buffer for roughly half of this column's encoded bytes, and the merge copies those bytes. Peak memory was not measured. Additional CPU concurrency is explicitly part of this implementation and comparison; this is not a single-thread engine comparison or a CPU-efficiency claim. The 4096/8MiB gates are conservative, not measured optimal crossovers.

## Paired local durable measurements

Each series: six alternating baseline/candidate process pairs, 24 fresh checked fixtures per case, one operation per sample, no warmup. No builds/tests overlapped these measurements. SQLite is an unchanged same-process control. Percentages are median paired improvements derived from process medians, not ratios of aggregate medians. CPU frequency and external background load are uncontrolled. Case validation checks full expected row values and insertion count outside timing.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial refactored serial path | -6.313%, 0/6 | +1.380%, 4/6 | +12.427%, 5/6 | +0.035%, 3/6 |
| Final original small path | -0.420%, 2/6 | +0.693%, 5/6 | +11.137%, 6/6 | -0.096%, 2/6 |
| Independent final repeat | +0.430%, 4/6 | +0.575%, 3/6 | +11.002%, 6/6 | +1.094%, 4/6 |

The initial candidate factored the serial loop into a helper and validated budgets in a separate pass, including small batches. It was rejected as implemented because 1k regressed in all six pairs. The final version restores the original checked loop for small/light columns and adds a separate large-column branch. Both final series show about 11% improvement on 10k with all six wins; the small case changes sign between the two series and shows no consistent regression. The unchanged SQLite control changes by roughly -0.10% / +1.09% at 10k, materially less than the repeated Lin change.

Final-repeat aggregate process medians, milliseconds:

| Size | Lin baseline | Lin final | SQLite final control |
|---|---:|---:|---:|
| 1k | 3.148698 | 3.101396 | 1.860291 |
| 10k | 23.824542 | 21.260292 | 19.053614 |

Lin still loses these durable insert comparisons to SQLite. This is concrete progress toward, not proof of, the complete named-peer objective. No MSSQL/Kusto or refreshed full-peer superiority claim is made.

## Verification and provenance

Initial eight WAL integrity tests passed. Full workspace tests passed for the final production implementation. After adding an additional budget regression, all nine focused WAL integrity tests passed; production code before #[cfg(test)] was verified byte-identical to the measured final source, with SHA-256 in verified-source-sha256.json. New tests use an independent scalar sparse encoder to compare all bytes of 4101 mixed rows across odd worker boundaries, None/Some(empty), negative zero, NaN payload, infinity and tail coordinates; compare actual WAL payload bytes; decode the final frame; and inject thread-creation failure to verify identical serial fallback. A two-column fixture exceeding the shared 64MiB decoded budget verifies the existing WAL bytes and byte counter remain unchanged after rejection. This injects resource errors and does not claim actual OS exhaustion testing.

Baseline executable is the previously pinned row-major final binary, copied without rebuilding; before-persist.rs is byte-identical to its retained final-persist.rs snapshot. Intervening commits were benchmark evidence only. Candidate-persist.rs identifies the initial variant, final-persist.rs the measured final variant, and verified-persist.rs the final source including the additional test. Raw outputs and initial/final/repeat source and binary hashes are separate. run-pairs.py, run-final.py and run-repeat-final.py record exact invocations using pinned executable paths. The goal remains active.


</details>

### 124. 2026-10-04-row-major-wal-pack

Артефакты: [2026-10-04-row-major-wal-pack](../benches/results/2026-10-04-row-major-wal-pack).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-row-major-wal-pack/report.md).

# Row-major WAL column packing

Retained final implementation: for uniform batches of at least 4096 rows whose first row has no null cells, allocate typed columns from that first row and fill all columns in one traversal of each row. This removes the temporary flattened cell-reference buffer and repeated column traversal. Smaller, heterogeneous and null-first batches retain the existing conversion path. Wire format, field ordering and type coercions are unchanged.

Baseline: main 525b257261b6394307386f70db25f538f9ab8e99. Source snapshots and SHA-256 metadata distinguish the initial unrestricted candidate from the final size-gated candidate. Executables were pinned before timing; no builds or tests overlapped timing.

Each series consists of six alternating baseline/candidate process pairs. Each case uses 24 fresh fixtures, one operation per sample, no warmup. Reported gains are the median of the six paired percentage improvements, not the ratio of aggregate medians. SQLite is an unchanged control run in both executables. These timings apply to the existing local compare durable fixture and are not universal engine CPU measurements.

| Series | Lin durable 1k | SQLite control 1k | Lin durable 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial unrestricted | -0.597%, 3/6 | +1.213%, 4/6 | +1.292%, 5/6 | +0.247%, 4/6 |
| Independent unrestricted repeat | -0.693%, 2/6 | +1.649%, 4/6 | +3.144%, 6/6 | +1.009%, 4/6 |
| Final size gate | +0.087%, 3/6 | -0.951%, 2/6 | +1.885%, 6/6 | -0.247%, 1/6 |

The unrestricted implementation slightly regressed the small fixture twice, motivating the final size gate. The final large fixture consistently improved, while the small fixture is effectively unchanged. The 4096 threshold is a conservative gate, not a measured optimal crossover.

Final aggregate process-median timings: Lin 1k 3.240365 -> 3.119396 ms (paired gain is only 0.087%); Lin 10k 23.005239 -> 22.546396 ms. Final SQLite medians were 1.801646 ms and 18.331562 ms respectively. Lin still loses these durable write comparisons to SQLite; the full named-peer objective remains incomplete. This change does not establish MSSQL or Kusto performance.

Validation: all seven focused WAL integrity tests passed; full workspace tests passed for the final size-gated implementation. A new 4096-row regression checks exact serialized frame bytes against explicitly constructed columns, including null/default coercions, integers in float/time columns, negative zero, NaN payload and infinity. Existing field-union, shared text and replay tests passed. The initial test failure in tests-initial.log came from the frame test helper appending the expected frame after the actual frame; resetting the test file before comparison fixed the test setup. No production correction was needed for that failure.

Raw results: medians.json, repeat-medians.json, final-medians.json and per-process run.json/log files. run-pairs.py, run-repeat.py, run-final.py reproduce the benchmark invocation using the pinned executables; source/binary hashes are saved separately for the final variant.


</details>

### 125. 2026-10-04-row-worker-fallback

Артефакты: [2026-10-04-row-worker-fallback](../benches/results/2026-10-04-row-worker-fallback).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-row-worker-fallback/report.md).

# Large-row worker creation fallback

Baseline: 156e991. Large inserts formerly used scope.spawn, which panics if the OS cannot create a thread. Candidate uses Builder::spawn_scoped and handles its io::Error by constructing the right half serially. Parallel success retains original order and one captured timestamp. Actual worker panics are still propagated; this specifically handles thread creation failure. No public API, persistence format or durability policy changes.

A deterministic regression injects WouldBlock at the real tail-merge helper and verifies 2053 tail rows, original left prefix, ordered IDs, Unicode/last duplicate title fields and the same timestamp. This tests the fallback branch without exhausting system resources; a real OS resource exhaustion event was not induced. Existing parallel boundary/duplicate-ID/URI tests and full `cargo test --offline --workspace` passed. Both benchmark binaries built. `git diff --check` passed.

Six alternating independent baseline/candidate process pairs per workload, 24 fresh fixtures per process, one operation/sample and no warmup. Exact affected-count/value/readback checks passed. Setup/preparation excluded; thread creation/join and synchronous durable operation remain inside measured time. Builds and tests did not overlap timing.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | -0.03% | 3/6 | 0.927625 | 0.919969 |
| compare/insert_bulk_1k/sqlite | -1.42% | 2/6 | 0.813354 | 0.818125 |
| compare/insert_bulk_10k/lin | 1.68% | 5/6 | 9.669646 | 9.507427 |
| compare/insert_bulk_10k/sqlite | -0.16% | 2/6 | 10.678635 | 10.623844 |
| compare/durable_insert_1k/lin | 0.37% | 3/6 | 2.569562 | 2.549240 |
| compare/durable_insert_1k/sqlite | -0.59% | 2/6 | 1.385448 | 1.396979 |
| compare/durable_insert_10k/lin | 1.20% | 5/6 | 18.931135 | 18.730156 |
| compare/durable_insert_10k/sqlite | -0.16% | 3/6 | 13.791615 | 13.802958 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Small workloads are essentially unchanged/mixed. Larger workloads show modest positive changes; these do not establish that the error-handling branch causes acceleration. The change is retained for availability, with no observed consistent slowdown in these six process pairs. Thread resources and transient right-half row headers remain the tradeoff for successful parallel execution; fallback uses one execution context.

Candidate Lin vs SQLite: native 1k loses all six pairs (aggregate medians 0.919969 vs 0.818125 ms); native 10k wins all six (9.507427 vs 10.623844 ms); durable 1k/10k lose all six (2.549240 vs 1.396979 ms and 18.730156 vs 13.802958 ms). The full nine-engine goal remains incomplete, including the prior absent MSSQL/Kusto endpoints. This is a regression/availability check, not a fresh full peer matrix.


</details>

### 126. 2026-10-04-row-worker-low-threshold

Артефакты: [2026-10-04-row-worker-low-threshold](../benches/results/2026-10-04-row-worker-low-threshold).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-row-worker-low-threshold/report.md).

# Lower row-worker threshold: rejected

Baseline: d70fed1. Candidate changes only the parallel row-construction threshold from 4096 to 512. Thread creation/join stays inside operation timing; timestamp, validation order, worker-creation fallback and all persisted representations stay unchanged. Sources and binary SHA256 hashes are saved.

Six independent alternating baseline/candidate process pairs per workload; 24 fresh fixtures per process, one operation/sample, no warmup. Setup/schema/preparation excluded. Exact fixture/count/value/readback validation passed. No builds/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.51% | 2/6 |
| compare/insert_bulk_1k/sqlite | 0.29% | 4/6 |
| compare/insert_bulk_10k/lin | 0.28% | 4/6 |
| compare/insert_bulk_10k/sqlite | 0.68% | 4/6 |
| compare/durable_insert_1k/lin | -1.64% | 2/6 |
| compare/durable_insert_1k/sqlite | -2.57% | 2/6 |
| compare/durable_insert_10k/lin | -0.88% | 0/6 |
| compare/durable_insert_10k/sqlite | 0.38% | 4/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Neither small workload improved consistently, so starting a second worker at 1k does not close the SQLite gap in this evidence. Durable 1k SQLite control also slowed, limiting causal inference about its Lin slowdown. The 10k path uses two execution contexts in both variants; its timing changes do not directly measure benefit/cost of the lower threshold, and may reflect noise or compilation changes.

Decision: reject. src/exec.rs restored byte-for-byte to baseline, retaining threshold 4096 and the earlier OS thread-creation fallback. Two focused parallel-bulk tests passed; both benchmark binaries built. Full workspace tests were not run for this rejected one-line candidate. `git diff --check` passed after restoring source and normalizing log trailing blank lines.

The full nine-engine objective remains incomplete. This is evidence against lowering the threshold, not a performance improvement or full peer matrix refresh.


</details>

### 127. 2026-10-04-scoped-batch-embed

Артефакты: [2026-10-04-scoped-batch-embed](../benches/results/2026-10-04-scoped-batch-embed).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-scoped-batch-embed/report.md).

# Rejected scoped batch hashing embedding

Baseline main 2feb9a8, with retained parallel sparse WAL encoding. Baseline SHA256 was checked against the pinned final candidate metadata of that retained experiment. This experiment affects only HashingEmbedder::embed_batch: for at least 4096 texts and dimension at least 128, split the input into two halves, reuse scratch separately for each half, run the right half on one scoped worker and append its results after the left half. Thread creation failure falls back to serial processing; worker panics propagate. Third-party embedders and the Embedder trait defaults are unchanged. No pool or dependency was added, and worker creation/join and merging are included in insertion timing. Additional CPU concurrency and per-thread scratch/thread resources are part of the attempted change.

The first candidate embeds scoped setup within the trait method; the second puts the same parallel work behind a separate #[inline(never)] private function while retaining the existing small-batch loop. Both are rejected. The second candidate tested a code-structure hypothesis, not a proven cause for the first regression. These results do not establish whether thread startup, allocator contention, code layout or other factors caused the slowdown.

## Six-pair measurements for each candidate

For each variant: six alternating process pairs on both native and durable fixtures, 24 fresh samples per case, one operation/sample, no warmup. Exact existing fixture validation is outside timing. Baseline and candidate executables were pinned; builds/tests did not overlap timing. SQLite is an unchanged control. Gains are median paired improvements from six process medians; negative means slower. CPU frequency/background load are uncontrolled.

| Variant and mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial scoped branch Native | -3.763%, 0/6 | +1.073%, 5/6 | -4.435%, 0/6 | -1.035%, 1/6 |
| Initial scoped branch Durable | -4.458%, 0/6 | -0.997%, 2/6 | -7.149%, 0/6 | +0.229%, 4/6 |
| Outlined scoped function Native | -5.808%, 0/6 | -0.831%, 1/6 | -7.604%, 1/6 | -0.846%, 2/6 |
| Outlined scoped function Durable | -4.596%, 0/6 | +2.154%, 4/6 | -5.805%, 0/6 | -1.396%, 2/6 |

Lin consistently regressed on both sizes and both execution modes, with at most one win in any scenario. SQLite's smaller control shifts do not account for all of the Lin differences. Small batches never create the worker but also regress; no assembly/profile attribution was made. Do not retain either implementation or infer a win from split computation alone.

## Validation and restoration

Seven focused embedding tests passed for each candidate. Added coverage tests 4095/4096/4101 texts at dimensions 8/768, output order/length and exact float bits against individual embedding calls, contextual Greek and other Unicode, vertical-tab/Unicode whitespace, empty texts, and injected WouldBlock worker-creation fallback. The outlined variant additionally asserts each vector dimension. Existing independent dense-reference, sparse normalization and exhaustive bigram hash tests passed. Full workspace was not rerun because neither production variant is retained.

src/embed.rs was restored byte-for-byte from before-embed.rs; the added tests were restored too. Production behavior and all earlier retained improvements remain unchanged. Candidate snapshots, separate binary/source hashes, environment, test/build logs, run scripts and raw outputs identify both attempts. The full named-peer objective and durable SQLite gaps remain unresolved.


</details>

### 128. 2026-10-04-small-sparse-backfill

Артефакты: [2026-10-04-small-sparse-backfill](../benches/results/2026-10-04-small-sparse-backfill).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-small-sparse-backfill/report.md).

# Rejected small-batch sparse count backfill

Baseline main 66d3361. Candidate applies a single-pass sparse encoder only when the vector column has fewer than 4096 rows. Preserve decoded-budget/dimension checks and eight-float zero-block skipping, reserve a four-byte count header, write entries, then derive count from encoded-entry byte length divided by eight and patch the header. This avoids both a separate count scan and an increment on every entry. Existing large-batch classifier/encoder and serial large/light-column loop are retained. No new format, approximation, allocation table, dependency, CPU worker or durability tradeoff.

This differs from the rejected 2026-10-01 global single-pass experiment, which used a per-entry increment and affected 10k too; that earlier experiment had an initial 1k gain and a large-case regression. Here the actual loop only changes below 4096. That does not guarantee unchanged timing for the other code paths, and neither assembly nor causal profiling was performed.

## Durable comparisons

Two independent six-pair series using exactly the same pinned baseline/candidate binaries, alternating process order, 24 fresh checked fixtures per case, one operation per sample, no warmup. Builds/tests did not overlap timing. Existing full row/count validation is outside timing. SQLite is an unchanged control. Percentages are medians of paired percentage changes based on process medians, not aggregate-median ratios. Background load/CPU clocks are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | +2.004%, 4/6 | +0.215%, 3/6 | +0.543%, 3/6 | -0.528%, 2/6 |
| Independent repeat | -4.870%, 2/6 | -9.439%, 3/6 | -11.825%, 1/6 | +6.642%, 4/6 |

The first small gain does not reproduce. The repeat has substantial control shifts, so no isolated cause is assigned to the Lin changes, but it provides no evidence to retain the prototype. The larger case also regresses materially despite unchanged loop selection and an improving SQLite control. Reject; do not preserve a first-series win while discarding contradictory repeat results.

## Verification and restoration

All eleven focused WAL integrity tests and full workspace tests passed. New coverage compares single-pass bytes to the existing serial codec over 1001 mixed vectors, forcing buffer growth from a tiny prefix, None/Some(empty), negative zero, NaN payload, infinity and a partial final block. The actual frame payload is checked against the same reference. Direct decoded-budget failures check that a row rejected before emission preserves the previous prefix and that metadata-only overflow is still rejected. Existing scalar-reference parallel frame, checksum/truncation and shared-column budget/no-file-write tests passed.

Production src/persist.rs and the added test were restored byte-for-byte from before-persist.rs. Baseline executable hash matches the retained sparse-classifier candidate; intervening commits contain evidence only. Raw reports, source/binary hashes, test/build logs and exact scripts remain here. Previously confirmed WAL improvements and read matrix are unchanged. Small-write gaps and full named-peer proof remain unresolved; the goal stays active.


</details>

### 129. 2026-10-04-vector-dictionary-audit

Артефакты: [2026-10-04-vector-dictionary-audit](../benches/results/2026-10-04-vector-dictionary-audit).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-vector-dictionary-audit/report.md).

# Bit-exact vector dictionary size audit

Source: 172703c. Diagnostic prototype uses actual default embeddings produced by Db insert with the Rust compare fixture's title/body/ID/URI/wing/layer shape, 768 dimensions and scalar wing/ts index. Fixed timestamp 1700000000000 avoids time drift; it does not enter embedding input. Three independent release processes produced identical statistics for 1000 and 10000 rows. This is byte-size/roundtrip evidence, not performance timing or a changed WAL format.

| Documents | Current sparse vector bytes | Prototype dictionary vector bytes | Vector-section reduction |
|---|---:|---:|---:|
| 1000 | 238,992 | 112,542 | 52.91% |
| 10000 | 2,539,664 | 1,180,574 | 53.51% |

Every measured vector has 4–6 distinct nonzero f32 bit patterns. At 1k: dictionary sizes 4/5/6 occur 773/224/3 times, total 28874 nonzero entries. At 10k: 8022/1906/72 times, total 307458 entries. These counts and sizes were identical across three processes.

Accounting: current sparse row = u32 dimension code + u32 count + (u32 index,u32 f32 bits) per nonzero. Proposed row = same eight header bytes + u8 dictionary length + four bytes per distinct value + (u16 index,u8 value-code) for dimensions <=65536, otherwise u32 index. Values are dictionary entries of exact bits, not quantized numbers. -0.0 and NaN payloads remain distinct from +0.0. The trusted-data prototype encodes/decodes entries and asserts all f32 bits match, for every measured vector plus synthetic empty/all-positive-zero/negative-zero/NaN/Inf/repeated values. It is NOT a production decoder and has no untrusted-input safety validation or reader compatibility gate.

These totals cover vector-column rows only. They exclude other columns, frame headers/checksums, column metadata and write/fsync overhead. They do not establish a 53% reduction of full WAL, CPU benefit or end-to-end latency. Per-row dictionary search/encoding costs could offset byte savings. A dictionary-overflow fallback (>255 distinct values) is sketched in the prototype but was not triggered by the measured workload or validated for roundtrip.

Next justified candidate: versioned dictionary codec with raw escape for poorly compressible vectors, preserving None/Some(empty), every float bit and arbitrary dimensions; bounded decoded allocations and ordered-index/value-code checks; reading all existing frame/codec versions; checksum/truncation/malformed-input tests; durable fixture reopen and timing against current source/SQLite. Existing reader rejection of new codecs must be deliberate and documented. No speedup may be claimed until those checks and timings pass.

The temporary existing profile_durable example was saved and restored byte-for-byte to HEAD BEFORE running the pinned audit binary. Build passed. Original/instrumented example and source/binary SHA256 hashes are saved. Production source/WAL format are unchanged; no full workspace tests were needed for the reverted diagnostic-only source. `git diff --check` passed after log trailing whitespace normalization. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and unavailable MSSQL/Kusto endpoint evidence.


</details>

### 130. 2026-10-04-vector-dictionary-buffered

Артефакты: [2026-10-04-vector-dictionary-buffered](../benches/results/2026-10-04-vector-dictionary-buffered).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-vector-dictionary-buffered/report.md).

# Buffered dictionary vector encoder: rejected

Baseline: 0b31508, production codec 3. Candidate starts from the prior rejected experimental codec 4/tag 8 source, but replaces its encoder with block-zero skipping and a single value-code lookup per nonzero entry. An inline SmallVec buffer of 64 (u32 index,u8 code) pairs feeds final serialization without another full dense-vector scan or repeated dictionary lookup. Larger entry sets spill to temporary heap storage. Dictionary overflow still re-encodes raw pairs. Float bits, dimension-dependent positions, None/empty distinction, CRC/fsync and codec rejection/reader semantics remain those of the prototype. No experimental codec was previously deployed.

Six independent alternating baseline/candidate pairs; 24 fresh one-operation samples per size/process, no warmup. Setup/schema/preparation excluded; normal synchronous durable insertion including encode/write/sync measured. Exact affected-count/value/readback checks passed. Builds/tests did not overlap timing.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -11.85% | 0/6 |
| compare/durable_insert_1k/sqlite | 1.19% | 4/6 |
| compare/durable_insert_10k/lin | -15.38% | 0/6 |
| compare/durable_insert_10k/sqlite | 0.73% | 4/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). Lin loses all six pairs at both sizes while SQLite controls are slightly faster/mixed. Decision: reject. These runs compare directly against production codec 3, not side-by-side against the earlier dictionary encoder. The smaller recorded regression than the earlier experiment is not a controlled measurement of improvement between encoder implementations. Whole-operation timing does not isolate dictionary lookup, buffering, memory traffic or fsync as a cause.

Eight focused WAL integrity tests passed, including legacy sparse/checksum cases and dictionary exact -0/NaN/Inf/presence/empty values, 65536/65537 width boundaries, raw overflow escape, truncation, malformed codes/tables/indices/budget. Benchmark builds passed. Full workspace suite was not rerun for this rejected buffer variant; the prior dictionary prototype's full suite is not claimed as validation of this changed encoder. Candidate source/test snapshots identify the provisional codec-number assertion (4) along with its other integration checks.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved hashes and raw samples retain the experiment. `git diff --check` passed after log trailing-blank normalization. No dictionary codec changes remain in production. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements.

A distinct next compression candidate may reduce coordinate width without value dictionaries or extra per-row buffers, retaining exact bits and dimension-based fallback. It still requires reader compatibility/safety tests and measured durable latency; smaller bytes alone are insufficient.


</details>

### 131. 2026-10-04-vector-dictionary-codec

Артефакты: [2026-10-04-vector-dictionary-codec](../benches/results/2026-10-04-vector-dictionary-codec).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-vector-dictionary-codec/report.md).

# Dictionary vector WAL codec prototype: rejected

Baseline: f0d424d. Candidate adds internal codec 4/tag 8 in the checksummed LIN\x06 envelope, retaining readers for codecs 1/2/3. Large sparse columns (>=256 rows) use dictionary-eligible rows; other existing paths remain. Each row uses exact f32 bits, u8 dictionary length/codes and u16 positions for dimensions <=65536 (u32 above that). Dictionary overflow (>255 values) or unprofitable encoding uses marker zero plus raw u32 position/bits pairs. None and Some(empty) remain distinct. CRC/fsync policy unchanged. Old binaries reject codec 4; that backward reader limitation was recognized, and this candidate is NOT deployed.

The earlier size audit supports vector-section reduction around 53%, not a full-WAL or latency claim. This prototype scans vectors for counts/dictionary discovery/encoding and searches dictionaries for codes. Those costs are not separately timed here; whole-operation measurements cannot conclusively attribute the regression to a specific loop or cache effect.

Six alternating independent baseline/candidate process pairs, 24 fresh one-operation samples per case/process, no warmup. Setup/schema/preparation excluded; encoding/write/durability sync remain timed. Exact affected-count/value/readback checks passed. No builds/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -20.66% | 0/6 |
| compare/durable_insert_1k/sqlite | -0.24% | 3/6 |
| compare/durable_insert_10k/lin | -32.60% | 0/6 |
| compare/durable_insert_10k/sqlite | -0.37% | 2/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). Lin regressed in every pair at both sizes while SQLite controls stayed near flat/mixed. Decision: reject this encoder/format tradeoff. Smaller byte estimates do not satisfy the requested latency goal. A future implementation must address measured encoding cost before a new format is justified; this result does not rule out every dictionary design.

Validation: existing sparse integrity tests passed. Two new dictionary tests passed, covering exact negative-zero/NaN/Inf values, empty/zero/None presence, 65536/65537 position-width boundaries, raw escape after dictionary overflow, all payload truncation prefixes, malformed dictionary codes/tables/order/out-of-range indices and decoded-size budget. Full workspace initially failed only the large-frame codec-number assertion (3 vs 4); that assertion was updated while retaining its frame/reopen/replay/value checks, and the complete workspace suite then passed. Initial failure and final successful logs retained. Both benchmark binaries built. Tests cover synthetic trusted/malformed examples; no broad fuzz campaign was run.

Sparse/dictionary expanded vector allocation follows the existing 64MiB budget; encoded frame limit remains 16MiB. The standalone dictionary helper also rejects oversized dimensions before allocating its values. These limits refer to the new expanding vector path, not a claim that every legacy raw/text allocation shares one global 64MiB ceiling.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved source/test copies and SHA256 hashes identify the exact prototype. No new codec has been published in production source. Log trailing blank lines normalized, raw JSON retained. `git diff --check` passed. The full nine-engine objective remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint evidence.


</details>

### 132. 2026-10-04-wal-narrow-coordinates

Артефакты: [2026-10-04-wal-narrow-coordinates](../benches/results/2026-10-04-wal-narrow-coordinates).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-wal-narrow-coordinates/report.md).

# Narrow sparse WAL coordinates: rejected as unconfirmed

Baseline: 1a2dc09. Candidate adds internal codec 5/tag 9 within the checksummed LIN\x06 envelope, with old codecs 1/2/3 still readable. Sparse columns >=256 rows encode coordinates as u16 for dimension <=65536, otherwise u32. Float bits, ordering, None/Some(empty), CRC, fsync and decoded sparse vector budget unchanged. No value dictionary or additional coordinate buffer. Old binaries reject codec 5; no such format was previously deployed.

Six alternating independent baseline/candidate process pairs per series, 24 fresh fixtures per case/process, one operation/sample and no warmup. Setup/schema/preparation excluded; normal synchronous durable operation remains timed. Exact affected-count/value/readback checks passed. No builds/tests overlapped timing. Independent repeat used the same pinned binaries without rebuilding.

Mixed-size series

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | 8.13% | 5/6 |
| compare/durable_insert_1k/sqlite | -0.09% | 3/6 |
| compare/durable_insert_10k/lin | 0.94% | 4/6 |
| compare/durable_insert_10k/sqlite | 0.06% | 4/6 |

Independent 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | 1.28% | 4/6 |
| compare/durable_insert_1k/sqlite | 2.25% | 5/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Initial small-batch gain looked promising with a flat SQLite control. Independent 1k repeat does not reproduce a comparably strong isolated gain: Lin +1.28% in 4/6 pairs, SQLite +2.25% in 5/6. Large-batch indication is weak/mixed. These data do not establish a generally useful durable latency improvement and do not close the SQLite gap. Decision: reject the unconfirmed format tradeoff; smaller coordinates alone are not acceptance proof for the requested performance goal.

Narrow entry bytes decrease from 8 to 6 (coordinate plus exact f32 bits), a 25% per-entry reduction. Per-vector headers/other columns/frame bytes remain, and full WAL byte savings were not independently measured here. Values above the dimension boundary keep eight-byte entries.

Validation: eight focused WAL integrity tests passed. New cases cover bit-exact -0/NaN/Inf, all-zero/empty/missing vectors, last-coordinate width boundaries 65536/65537, all frame truncation prefixes, checksum flips, old decoder rejecting the new tag, trailing payload, decoded dimension budget, oversized counts and invalid/duplicate indices. Full `cargo test --offline --workspace` passed after the integration codec-number assertion was updated to 5; its one-frame/reopen/replay/value checks remained. Both benchmark binaries built. No broad fuzz campaign was run.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved source/test copies and hashes identify the complete candidate, including the provisional codec-number assertion. No new format remains in production. `git diff --check` passed after log trailing-blank normalization. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint measurements.


</details>

### 133. 2026-10-04-wal-paired-write

Артефакты: [2026-10-04-wal-paired-write](../benches/results/2026-10-04-wal-paired-write).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-wal-paired-write/report.md).

# Paired sparse WAL writes: rejected

Baseline: 3f5e556. Candidate packs index-u32 and raw f32 bits into one little-endian u64 append rather than two u32 appends. Exact byte order, negative-zero/NaN payloads, sparse row selection, codec, checksums and fsync guarantees remain unchanged. Both sources and binary hashes saved.

Each series uses six independent alternating baseline/candidate process pairs, 24 fresh one-operation samples per case/process, no warmup. Preparation/schema/setup excluded. Exact fixture/count/value/readback checks passed. No builds/tests overlapped timed measurements.

Initial mixed-size run

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -0.40% | 3/6 |
| compare/durable_insert_1k/sqlite | -7.54% | 0/6 |
| compare/durable_insert_10k/lin | -20.59% | 1/6 |
| compare/durable_insert_10k/sqlite | -22.66% | 0/6 |

Independent 10k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_10k/lin | 2.49% | 5/6 |
| compare/durable_insert_10k/sqlite | -1.84% | 2/6 |

Independent 1k repeat

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -2.47% | 1/6 |
| compare/durable_insert_1k/sqlite | -0.02% | 3/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). The initial mixed-size run is strongly confounded by unmodified SQLite controls slowing in all six pairs, especially 10k. Those raw data are retained, not discarded. Independent single-size repeats were run with the SAME pinned binaries to resolve that uncertainty, without rebuilding or changing the candidate.

The 10k repeat improved Lin 2.49% in 5/6 pairs while SQLite control slowed 1.85%. The 1k repeat regressed Lin 2.47% in 5/6 pairs while SQLite control stayed essentially flat (-0.02%). Decision: reject this small-write tradeoff. The objective includes the unresolved small SQLite gap, and this variant worsens it in the better-controlled repeat. Whole-operation wall times do not establish an intrinsic CPU cause for the regression.

Two focused sparse-vector integrity tests passed (exact -0/NaN/Inf bits, missing/empty vectors, partial tails/block boundaries, dense fallback, checksums/truncation and malformed bounds). Both benchmark binaries built. Full workspace tests were not run for this rejected candidate. Production src/persist.rs restored byte-for-byte to baseline. Log trailing blank lines normalized; raw JSON unchanged. `git diff --check` passed. No production code changes remain.

The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint proof. This is a rejected experiment with follow-up evidence, not a universal acceleration or full peer matrix refresh.


</details>

### 134. 2026-10-04-wal-vectored-write

Артефакты: [2026-10-04-wal-vectored-write](../benches/results/2026-10-04-wal-vectored-write).

<details>
<summary>report.md</summary>

Источник: [report.md](../benches/results/2026-10-04-wal-vectored-write/report.md).

# Rejected WAL vectored-write experiment

Baseline main 1edcdca. Prototype replaces four write_all calls per checksummed frame with write_vectored over stack IoSlice descriptors, advancing after short writes, retrying Interrupted and propagating WriteZero/other I/O errors. Payload, CRC, envelope and durable_sync ordering are unchanged. No extra payload copy or format change.

Eight focused WAL integrity tests passed, including the added injected short-write/interruption/WriteZero/BrokenPipe checks and existing frame/checksum/truncation/vector tests. Benchmark candidate built successfully. Full workspace was not rerun because the benchmark rejected the production change.

Six alternating baseline/candidate process pairs, 24 fresh samples per case, one operation/sample, no warmup, existing compare checked fixtures. Executables were pinned; Lin and SQLite both used their existing durable contract. These are end-to-end local API timings, not isolated syscall CPU timings. Each pair gain is computed from process medians and then the median paired percentage is reported; aggregate medians are listed separately. SQLite is an unchanged control.

| Case | Baseline median ms | Candidate median ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| compare/durable_insert_1k/lin | 3.462875 | 3.338084 | +1.021% | 3/6 |
| compare/durable_insert_1k/sqlite | 1.788750 | 1.798219 | -0.732% | 1/6 |
| compare/durable_insert_10k/lin | 22.598427 | 22.756584 | -0.058% | 3/6 |
| compare/durable_insert_10k/sqlite | 18.485709 | 18.384355 | +0.071% | 3/6 |

Neither fixture showed consistent gains (3/6 wins each). Reject the extra write loop complexity rather than infer a win from lower call count. This result does not prove syscalls are universally insignificant; the experiment only fails to demonstrate an improvement in the measured fixtures. The complete production source and new test were restored byte-for-byte to baseline. Candidate source, test/build logs, pinned executable hashes, raw process results and summaries remain here for review. The prior retained row-major optimization remains in main. Durable SQLite gaps and the full named-peer objective remain unresolved.


</details>

### 135. 2026-10-05-durable-phase-refresh

Артефакты: [2026-10-05-durable-phase-refresh](../benches/results/2026-10-05-durable-phase-refresh).

<details>
<summary>README.md</summary>

Источник: [README.md](../benches/results/2026-10-05-durable-phase-refresh/README.md).

# Current durable insert phase diagnostics

Production source: a7ff491, including retained four-way sparse WAL encoder and parallel sparse-size classifier. Temporary timers built into a separate pinned release benchmark binary. Production src/exec.rs and src/persist.rs restored byte-for-byte before running diagnostics. This commit ships evidence only.

Three independent processes, each 24 fresh measured fixtures per 1k/10k Full durable insertion plus benchmark preflight (27 diagnostic events each size/process). Existing fixture validation completes and every run status is complete. One operation per sample, zero warmup, no build/test overlap. Raw phase observations, binary/source SHA-256 and timer patch retained. The table gives medians of three process medians in milliseconds. Classifier time is nested within encoding: do not sum it again. Separate phase medians cannot be added/subtracted to derive exact total or causal gain. Logging perturbs execution; these are bottleneck diagnostics, not new peer superiority results.

| Phase | 1k ms | 10k ms |
|---|---:|---:|
| build_ns | 0.3468 | 3.3979 |
| checks_ns | 0.1858 | 2.7064 |
| embed_ns | 0.5376 | 6.6995 |
| index_ns | 0.1670 | 1.6476 |
| fts_ns | 0.2240 | 2.8532 |
| maps_ns | 0.0451 | 0.7131 |
| pack_ns | 0.3450 | 4.2105 |
| classify_ns | 0.1512 | 1.2422 |
| encode_ns | 0.9295 | 6.1535 |
| checksum_ns | 0.0181 | 0.1923 |
| write_ns | 0.0786 | 0.3708 |
| sync_ns | 1.4489 | 1.5452 |

Payloads remain 336968 bytes at 1k and 3558340 at 10k. The current 1k run has sync about 1.449 ms, encoding 0.929 ms, embedding 0.538 ms, packing 0.345 ms and row build 0.347 ms. Next implementation target is the small-batch vector encoding/packing path; normalization-only changes address a fraction of embedding and the prior half-density experiment was flat. Do not weaken Full-sync to close the SQLite gap. Large-batch results and timings from the previous day are not paired causal comparisons: ambient throughput shifts materially, so future candidates need alternating controls and independent repeats.

The full named-peer objective remains active. This diagnostic does not establish MSSQL/Kusto comparisons or close small SQLite insertion gaps; those remain outstanding.


</details>

### 136. 2026-10-05-sparse-size-reserve

Артефакты: [2026-10-05-sparse-size-reserve](../benches/results/2026-10-05-sparse-size-reserve).

<details>
<summary>README.md</summary>

Источник: [README.md](../benches/results/2026-10-05-sparse-size-reserve/README.md).

# Reusing sparse-size estimate for WAL buffer reserve: rejected after repeat

Baseline source 5001867 (production identical to retained 4926d9c). Candidate adds an output size to the existing sparse classifier, reuses its exact computed byte count in the selected sparse column to reserve buffer capacity, and preserves the Some(empty) early return with a zero size hint. No duplicate size scan, codec/schema/order/hash/float bits or Full-sync change. Size hint is ignored above MAX_RECORD to avoid unbounded reservation for rejected records. Large classifier remains parallel; sparse encoder remains four-way.

Ten existing focused persist unit tests pass, covering serialized reference bytes, worker fallback, sparse decoded budget and recovery. Benchmark build succeeds. Each round uses six alternating process pairs per mode, 24 fresh fixtures per case, one operation per sample, no warmup. Same pinned binary pair in both rounds; hashes retained. No tests or builds overlap timing. Positive gain is faster, wins count process medians.

| Case | First paired gain / wins | Repeat paired gain / wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | +2.414% / 4/6 | -1.987% / 3/6 |
| compare/insert_bulk_1k/sqlite | +4.453% / 5/6 | -13.053% / 1/6 |
| compare/insert_bulk_10k/lin | +2.344% / 4/6 | -13.099% / 2/6 |
| compare/insert_bulk_10k/sqlite | +4.536% / 4/6 | -8.019% / 2/6 |
| compare/durable_insert_1k/lin | +2.687% / 3/6 | -3.799% / 2/6 |
| compare/durable_insert_1k/sqlite | +4.353% / 3/6 | +1.892% / 4/6 |
| compare/durable_insert_10k/lin | +20.337% / 5/6 | +3.165% / 3/6 |
| compare/durable_insert_10k/sqlite | +5.402% / 4/6 | +9.426% / 4/6 |

The initial durable 10k gain does not reproduce: repeat wins only 3/6, control improves more, and durable 1k regresses. Native cases also shift despite not using the changed WAL reserve path; do not attribute all variation to the candidate. Reject rather than retaining a first-round gain. No full workspace run performed for this rejected change. Production src/persist.rs restored byte-for-byte; candidate implementation preserved here only. The all-named-peer goal remains incomplete, including SQLite small insertion gaps and missing MSSQL/Kusto endpoints.


</details>

## Локальные прогоны

`.airbug-bench` — локальные, обычно игнорируемые Git артефакты. Ссылки доступны в этом checkout; перенос одного Markdown без этих файлов не сохраняет сырые результаты.

- [.airbug-bench/ci/run.json](../.airbug-bench/ci/run.json): статус `complete`, 80 cases, 640 observations; environment `{"arch": "aarch64", "os": "macos"}`.

- [.airbug-bench/lin-compare/run.json](../.airbug-bench/lin-compare/run.json): статус `failed`, 0 cases, 0 observations; environment `{"arch": "aarch64", "host": "tmtl-macbook-pro-m4.local", "kernel": "25.6.0", "os": "macos"}`.

- [.airbug-bench/smoke/run.json](../.airbug-bench/smoke/run.json): статус `complete`, 1 cases, 8 observations; environment `{"arch": "aarch64", "os": "macos"}`.

## Все сохранённые case ID

Каталог содержит каждый уникальный идентификатор из сохранённых Airbug run.json; пример контракта дан из одного прогона. Один ID может иметь другие параметры в других сериях. Peer API cases дополнительно описаны в исходных матрицах.

| Case ID | Параметры примера | Lifecycle примера | Пример источника |
|---|---|---|---|
| `compare/append_log_10k/duckdb` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_10k/lin` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_10k/mysql` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_10k/postgres` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_10k/sqlite` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_1k/duckdb` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_1k/lin` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_1k/mysql` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_1k/postgres` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/append_log_1k/sqlite` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/cold_reopen_5k/lin` | param.rows=5000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/delete_1row_100k/lin` | param.rows=100000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/delete_1row_100k/sqlite` | param.rows=100000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/delete_1row_10k/lin` | param.rows=10000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/delete_1row_10k/sqlite` | param.rows=10000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/delete_1row_1k/lin` | param.rows=1000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/delete_1row_1k/sqlite` | param.rows=1000; samples=24; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) |
| `compare/durable_append_10k/lin` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_append_10k/sqlite` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_append_1k/lin` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_append_1k/lin_normal_unsync` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_append_1k/sqlite` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_insert_10k/lin` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_insert_10k/sqlite` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_insert_1k/lin` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/durable_insert_1k/sqlite` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_eq/duckdb` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_eq/lin` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_eq/mysql` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_eq/postgres` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_eq/sqlite` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_range/duckdb` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_range/lin` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_range/mysql` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_range/postgres` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/filter_range/sqlite` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/fts_hybrid_common/lin` | param.candidates=1000; param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-fts-phases/run.json) |
| `compare/fts_lex_common/lin` | param.candidates=1000; param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-fts-phases/run.json) |
| `compare/fts_lex_miss/lin` | param.hits=0; param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-fts-phases/run.json) |
| `compare/fts_lex_selective/lin` | param.hits=1; param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-fts-phases/run.json) |
| `compare/graph_depth_1/lin` | param.depth=1; param.rows=5000; samples=8; warmup_ns=10000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../.airbug-bench/ci/run.json) |
| `compare/graph_depth_2/lin` | param.depth=2; param.rows=5000; samples=8; warmup_ns=10000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../.airbug-bench/ci/run.json) |
| `compare/graph_depth_3/lin` | param.depth=3; param.rows=5000; samples=8; warmup_ns=10000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../.airbug-bench/ci/run.json) |
| `compare/group_commit_16/lin_grouped_full` | param.commits=1; param.statements=16; samples=8; warmup_ns=10000000; work.count=16; work.unit=statements | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/group_commit_16/lin_sequential_full` | param.commits=16; samples=8; warmup_ns=10000000; work.count=16; work.unit=commits | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/hot_reopen_5k/lin` | param.rows=5000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/insert_bulk_10k/duckdb` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_10k/lin` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_10k/mysql` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_10k/postgres` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_10k/sqlite` | param.rows=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_1k/duckdb` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_1k/lin` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_1k/mysql` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_1k/postgres` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_bulk_1k/sqlite` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/insert_native_10k/duckdb_appender` | param.rows=10000; samples=12; warmup_ns=0; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-absolute-timestamps/native-bulk-clean/run.json) |
| `compare/insert_native_10k/mysql_batch` | param.rows=10000; samples=8; warmup_ns=0; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-02-mysql-owned-native/final/run-1/run.json) |
| `compare/insert_native_10k/postgres_copy` | param.rows=10000; samples=8; warmup_ns=0; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-01-pg-validated-insert/run-1/run.json) |
| `compare/insert_native_1k/duckdb_appender` | param.rows=1000; samples=12; warmup_ns=0; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-absolute-timestamps/native-bulk-clean/run.json) |
| `compare/insert_native_1k/mysql_batch` | param.rows=1000; samples=8; warmup_ns=0; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-02-mysql-owned-native/final/run-1/run.json) |
| `compare/insert_native_1k/postgres_copy` | param.rows=1000; samples=8; warmup_ns=0; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-01-pg-validated-insert/run-1/run.json) |
| `compare/insert_phase_10k/lin_full` | param.embed=1; param.rows=10000; param.scalar_index=1; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/insert_phase_10k/lin_no_embed` | param.embed=0; param.rows=10000; param.scalar_index=1; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/insert_phase_10k/lin_no_embed_no_scalar_index` | param.embed=0; param.rows=10000; param.scalar_index=0; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts` | param.embed=0; param.fts=0; param.rows=10000; param.scalar_index=0; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/0.4.0-insert-phases-repeat/run.json) |
| `compare/join_filter/duckdb` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_filter/lin` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_filter/lin_cursor` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_filter/mysql` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_filter/postgres` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_filter/sqlite` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_inner/duckdb` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_inner/lin` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_inner/lin_cursor` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/join_inner/lin_cursor_projected` | param.orders=10000; param.users=1000; samples=8; warmup_ns=10000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/join_inner/mysql` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_inner/postgres` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_inner/sqlite` | param.orders=10000; param.users=1000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/join_phase/lin_cursor_open` | param.orders=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/join_phase/lin_cursor_scan_project` | param.orders=10000; samples=100; warmup_ns=200000000; work.count=10000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/materialize/duckdb` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/materialize/lin` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/materialize/mysql` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/materialize/postgres` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/materialize/sqlite` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/plan_dnf_10/lin` | param.or_groups=10; samples=8; warmup_ns=10000000 | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/plan_dnf_14/lin` | param.or_groups=14; samples=8; warmup_ns=10000000 | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/plan_dnf_6/lin` | param.or_groups=6; samples=8; warmup_ns=10000000 | fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64 | [run.json](../benches/results/2026-10-01-native-core/run.json) |
| `compare/point_get/duckdb` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/point_get/hashmap` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/point_get/lin` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/point_get/mysql` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/point_get/postgres` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/point_get/sqlite` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/reader_snapshot/lin` | param.rows=10000; samples=8; warmup_ns=10000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../.airbug-bench/ci/run.json) |
| `compare/reopen_phase_5k/rebuild_fts` | param.rows=5000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/reopen_phase_5k/rebuild_row_maps` | param.rows=5000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/reopen_phase_5k/rebuild_scalar_indexes` | param.rows=5000; samples=100; warmup_ns=200000000; work.count=5000; work.unit=rows | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-core-phases/run.json) |
| `compare/text_substr/duckdb` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/text_substr/lin` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/text_substr/mysql` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/text_substr/postgres` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/text_substr/sqlite` | param.n=10000; samples=100; warmup_ns=200000000 | shared lazy process fixture; setup/borrow/drop excluded; output drop included | [run.json](../benches/results/0.4.0-full-70/run.json) |
| `compare/update_1row_100k/lin` | param.rows=100000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/update_1row_100k/sqlite` | param.rows=100000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/update_1row_10k/lin` | param.rows=10000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/update_1row_10k/sqlite` | param.rows=10000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/update_1row_1k/lin` | param.rows=1000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/update_1row_1k/sqlite` | param.rows=1000; samples=8; warmup_ns=0; work.count=1; work.unit=stmts | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) |
| `compare/wal_ship_1k/lin` | param.rows=1000; samples=100; warmup_ns=200000000; work.count=1000; work.unit=rows | fresh input per operation; input setup/drop and output drop excluded; chunks of <=64 | [run.json](../benches/results/0.4.0-full-70/run.json) |

## История сообщений диалога

Ниже сохранены сообщения ассистента о бенчмарках из локальной истории этого чата, в порядке времени. Это включает планы, предварительные результаты и исправления; окончательное решение следует сверять с отчётом и повтором соответствующей серии. Сырые команды, приватные пользовательские сообщения и служебные рассуждения не копируются.

<details>
<summary>1. 2026-09-30T22:09:56.341Z</summary>

Посмотрю устройство БД в проекте, текущие тесты и бенчмарки, затем предложу конкретные улучшения: что исправить, что измерить и что добавить.

</details>

<details>
<summary>2. 2026-09-30T22:10:20.073Z</summary>

В сохранённом отчёте видны три кандидата для исследования: удаление одной строки дорожает с размером коллекции, создание reader стоит около 13 мс, hybrid search — около 17 мс. Проверю код этих путей и условия бенчмарков: цифры пока относятся к старому прогону.

</details>

<details>
<summary>3. 2026-09-30T22:11:42.066Z</summary>

Я бы развивал Lin прежде всего как **надёжную встраиваемую БД с одним writer, дешёвыми readers и быстрыми пайпами**. По коду уже есть несколько конкретных точек роста.

**Что исправить первым**

1. **Undo для update/delete копирует всю БД.** В [exec.rs](../src/exec.rs#L788) любой не append-only запрос делает `mem_backup()`. Изменение одной строки поэтому зависит от общего объёма данных. В Store уже есть методы обратного восстановления строк — стоит подключить журнал изменений: старые строки, перемещённые позиции, изменения индексов. Обязательно проверить откат группы, где ошибка возникает после нескольких успешных операций.

2. **Планировщик может взрываться на сочетании AND/OR.** [DNF](../src/index.rs#L296) строит декартово произведение ветвей без видимого ограничения. Нужен бюджет раскрытия; при превышении — более простой план с остаточным фильтром. Сложный предикат должен замедлять запрос предсказуемо.

3. **Усилить контроль повреждений и ошибок durability.** WAL проверяет framing и декодирование, но в просмотренном формате нет checksum. Добавил бы checksum версии фрейма и тесты повреждения середины WAL. Также [atomic_write](../src/persist.rs#L492) игнорирует ошибку fsync директории — для строгой durability это стоит исправить.

**Что улучшить после этого**

| Участок | Что сейчас | Что предложил бы |
|---|---|---|
| Reader | `reader()` глубоко копирует состояние | Разделяемые неизменяемые блоки через `Arc`, copy-on-write при записи |
| Vector/hybrid | Перебор embeddings и полная сортировка результатов | Сначала bounded top-k и компактное хранение векторов; ANN — после измерений |
| Хранение строк | Общие `Row` плюс специальные SoA для docs/orders/users | Единое хранение колонок по схеме коллекции, чтобы ускорения работали и для пользовательских коллекций |
| Checkpoint | Self-contained snapshot, cold cache дополнительно | Измерить пиковую память, объём записи и паузу writer; затем решать вопрос сегментов и инкрементальных snapshots |

Для reader важно измерять обе стороны: создание snapshot станет дешевле, но удерживаемые readers могут увеличить память и стоимость последующих записей.

**Что побенчить**

В [сохранённом отчёте](../bench-report.md) delete одной строки занимает примерно **34 мкс на 1k** и **159 мкс на 10k**, reader — **13,4 мс**, hybrid — **17,1 мс**. Это старые результаты, свежий прогон я не запускал. Причём delete-бенч использует `DropPolicy::InsideTiming`, поэтому цифра включает уничтожение fixture и не доказывает стоимость самого удаления.

Я бы добавил такую матрицу:

- **Update/delete:** 1k / 10k / 100k / 1M строк; изменение одной и 1% строк. Раздельно parse/plan, undo, mutation, indexes/FTS, WAL/fsync, teardown.
- **Readers:** создание и клонирование reader, первая строка, RSS; 1 / 8 / 32 readers при продолжающейся записи.
- **Поиск:** lex/vec/hybrid, размеры корпуса, размерность вектора, top-10/100, селективность фильтра. Для ANN дополнительно recall относительно точного поиска.
- **Durable запись:** Full/Normal, группы 1/16/256, throughput и настоящие p50/p95/p99 отдельных commits, включая checkpoint.
- **Reopen:** одинаковый объём данных с коротким и длинным WAL, с cold cache и без; время и пиковая память.
- **Смешанная нагрузка:** чтение + update/delete + search на протяжении длительного прогона. Так видны рост памяти, деградация индексов и паузы обслуживания.

Сравнительные бенчи тоже стоит уточнить: prepared против prepared, одинаковый объём материализации, одинаковая durability. Для bulk добавить оптимальные пути конкурентов — COPY/batch/appender. Текущие сравнения полезны для конкретных API-сценариев, но недостаточны для общего вывода о скорости движков.

**Что добавить в продукт**

- **`explain analyze`:** реальные строки на каждом узле, кандидаты индекса, время, материализация и попадания в cache.
- **Бюджеты выполнения:** cancellation/deadline, лимиты промежуточных результатов, памяти join/sort и сложности плана.
- **`verify`/`doctor`:** проверка snapshot/WAL, поколений и согласованности производных индексов.
- **Crash/fault tests:** остановка процесса на этапах checkpoint, ошибки записи/fsync, усечённые и повреждённые файлы. Тесты выхода после commit уже есть; нужны проверки промежуточных состояний.

Мой порядок: **исправить измерение update/delete → ограничить DNF → подключить undo journal → проверить durability при сбоях → удешевить reader → оптимизировать vector top-k**. Это даст измеримый результат и укрепит основу для дальнейших возможностей.

</details>

<details>
<summary>4. 2026-09-30T22:12:55.090Z</summary>

Начну с первой очереди: исправлю измерение update/delete, ограничу раскрытие DNF, подключу журнал отката строк и проверю обработку ошибок durability. Затем прогоню тесты и целевые бенчмарки.

</details>

<details>
<summary>5. 2026-09-30T22:13:51.769Z</summary>

Нашёл ещё одну причину дорогого удаления: исполнитель использует `Vec::remove` и пересобирает все индексы и FTS коллекции. В Store уже есть swap-remove с точечным обновлением этих структур. Подключу его вместе с журналом отката, а для update уберу пересборку всех row maps.

</details>

<details>
<summary>6. 2026-09-30T22:18:46.536Z</summary>

Проверил сам benchmark harness и уточняю прежний вывод: `InsideTiming` включает уничтожение результата, а setup и уничтожение fixture уже исключены. Старые цифры нельзя списать на teardown. Для нового сравнения вынесу подготовку запросов из таймера и добавлю проверку, что запись действительно применилась.

PostgreSQL, MySQL и MongoDB уже запущены локально; pandas доступен в bundled Python. Нашёл также локальный образ Kusto — проверю возможность запуска.

</details>

<details>
<summary>7. 2026-09-30T22:24:58.536Z</summary>

Основные изменения готовы; идут проверки отката, индексов, FTS и durable reopen. Новый WAL использует CRC32, при этом чтение старых v1/v2 сохраняется.

Для сравнения всех движков добавляю одинаковые наборы данных и проверку содержимого результатов. Время подготовки данных и JSON-обмена с Lin будет вне измерений; серверные запросы будут включать round-trip и чтение результата.

</details>

<details>
<summary>8. 2026-09-30T22:32:19.852Z</summary>

Тесты выявили существующую ошибку числового индекса: float хранится как биты в integer key, поэтому indexed range возвращает не те строки, что scan. Исправлю её до бенчмарков — скорость при неверном результате не засчитывается. Проверки WAL и репликации уже прошли.

</details>

<details>
<summary>9. 2026-09-30T22:37:42.925Z</summary>

Первый расширенный прогон завершён: на 10k строк Lin быстрее SQLite, DuckDB, PostgreSQL, MySQL, MongoDB и pandas во всех шести проверенных сценариях чтения. Результаты сверены по значениям и количеству строк; было три повтора с новым worker или соединением.

Теперь отдельно измерю update/delete и планировщик на 1k–100k строк. MSSQL и Kusto остаются без живого прогона — для них нужен доступ к тестовым инстансам.

</details>

<details>
<summary>10. 2026-09-30T22:51:31.669Z</summary>

Сравнение до/после подтвердило большой выигрыш: update одной строки на 100k сократился примерно с 73 мс до 15 мкс, delete — со 149 мс до 27 мкс. Чтение и append в native harness тоже быстрее проверенных SQL-движков.

Цель ещё не достигнута полностью: SQLite пока выигрывает у Lin в bulk insert и некоторых single-row writes. Сохраню эти проигрыши в отчёте и продолжу оптимизацию записи.

</details>

<details>
<summary>11. 2026-09-30T22:58:51.525Z</summary>

На 100 тысячах строк нашлось конкретное узкое место: поиск подстроки в Lin занимает около 855 мкс, а в DuckDB — 489 мкс. Остальные пять проверенных сценариев чтения Lin выигрывает у всех шести доступных конкурентов. Теперь есть воспроизводимый проигрыш, по которому можно оптимизировать сканирование текста.

</details>

<details>
<summary>12. 2026-09-30T22:59:43.170Z</summary>

Исправления и проверки готовы: журнал отката вместо копирования всей БД для update/delete, ограничение разрастания DNF, корректное сравнение чисел в индексах и контрольная сумма WAL. В CI отделил тесты записи с дорогой подготовкой данных, чтобы автоматическая калибровка не затягивала прогон. Перемерю запись после последней оптимизации.

</details>

<details>
<summary>13. 2026-09-30T23:00:48.870Z</summary>

Сделал первый этап:

- Ускорил update/delete: на 100k строк **73 мс → 13 мкс** и **149 мс → 26 мкс**.
- Исправил числовые индексы, ограничил разрастание DNF, добавил checksum WAL.
- Добавил проверяемые сравнения и отдельный прогон записи в CI.

На 100k строк Lin выиграл **35 из 36 сравнений** с шестью доступными конкурентами. Остались проигрыши: DuckDB в поиске подстроки, SQLite в delete и bulk insert. MSSQL/Kusto требуют тестовых серверов. Цель пока не достигнута.

Тесты прошли; ограничения проверки и методики описаны в [отчёте](benchmark-results-2026-10-01.md).

**Новый WAL не читается старыми бинарниками** — writer и followers нужно обновлять вместе.

</details>

<details>
<summary>14. 2026-09-30T23:00:53.397Z</summary>

Продолжу с подтверждённого проигрыша DuckDB: проверю путь подсчёта подстроки и попробую ускорить его, затем повторю сравнение на тех же данных.

</details>

<details>
<summary>15. 2026-09-30T23:02:31.100Z</summary>

Первый вариант ускорил поиск, но пока не догнал DuckDB: около 724 мкс. Проверю хранение заголовков в непрерывном буфере — сейчас сканирование обращается к отдельным строкам, разбросанным в памяти.

</details>

<details>
<summary>16. 2026-09-30T23:04:55.604Z</summary>

Непрерывный буфер дал заметный выигрыш: первый замер — 234 мкс против прежних 855 мкс. Это буфер текста, а не кеш ответов: каждый запрос ищет подстроку заново. Проверки границ строк, Unicode и обновления буфера после записи проходят; жду ещё два независимых замера.

</details>

<details>
<summary>17. 2026-09-30T23:06:05.352Z</summary>

Закрыл проигрыш DuckDB в подсчёте подстроки на 100k строк: **Lin — 209 мкс, DuckDB — 485 мкс**, выигрыш **2,32×** в трёх независимых прогонах.

Добавил непрерывный буфер заголовков и проверки Unicode, границ строк, записи и отката. 52 теста прошли. Первый запрос после изменения данных оплачивает построение буфера; замеры выше — прогретые.

[Результаты](../benches/results/2026-10-01-substr-packed/report.md).

Цель остаётся активной: ещё нужны победы над SQLite в записи и проверка MSSQL/Kusto.

</details>

<details>
<summary>18. 2026-09-30T23:06:09.939Z</summary>

Продолжу с записи: проверю, где Lin тратит время при удалении и bulk insert, и сравню объём работы с SQLite. Затем оптимизирую подтверждённые узкие места.

</details>

<details>
<summary>19. 2026-09-30T23:08:17.418Z</summary>

Нашёл лишние аллокации в FTS: при каждой вставке и удалении токены копировались в отдельные строки. Для текста в нижнем регистре заменил их заимствованными срезами, сохранив нормализацию и удаление повторов. Также поправлю bulk benchmark: подготовка входных данных должна быть вне таймера у всех движков.

</details>

<details>
<summary>20. 2026-09-30T23:11:48.346Z</summary>

Убрал лишнее копирование токенов FTS и аллокации при регистрации bulk-строк. Исправил методику: подготовка входных данных теперь вне таймера у всех SQL-движков.

SQLite пока быстрее: bulk 10k — **10,6 мс против 14,2 мс**, delete 100k — **17 мкс против 25,5 мкс**. Замеры отдельных вариантов указывают на значительную стоимость embedding при вставке.

Регрессионные тесты прошли; долгий persistence stress-тест ещё не повторён. [Отчёт обновлён](benchmark-results-2026-10-01.md). Цель остаётся активной.

</details>

<details>
<summary>21. 2026-09-30T23:11:54.192Z</summary>

Продолжу оптимизацию удаления: проверю, можно ли убрать копирование строки, перемещаемой в освободившееся место. После изменения повторю проверки индексов и FTS и сравнение с SQLite.

</details>

<details>
<summary>22. 2026-09-30T23:13:55.829Z</summary>

Удаление лишней копии не дало убедительного выигрыша: delete на 100k по-прежнему около 25 мкс, SQLite — около 18 мкс. Уберу этот эксперимент. Для bulk insert добавил сравнение с DuckDB Appender: победа над его вставкой по одной строке ещё не доказывает преимущество над быстрым API загрузки.

</details>

<details>
<summary>23. 2026-09-30T23:15:54.546Z</summary>

В общем прогоне DuckDB Appender оказался быстрее Lin на bulk 10k: примерно 13,7 мс против 18,7 мс. Это новый подтверждённый ориентир для оптимизации. Запустил и оставшийся stress-тест FTS: ускорил только загрузку его фикстуры, сохранив все 900 отдельных удалений и проверки после checkpoint.

</details>

<details>
<summary>24. 2026-09-30T23:17:27.638Z</summary>

Добавил сравнение с **DuckDB Appender** с проверкой всех записанных значений. На bulk 10k Lin пока проигрывает: **18,8 мс**, SQLite — **14,4 мс**, DuckDB Appender — **13,7 мс**.

Эксперимент с копированием при delete убрал: убедительного ускорения не получилось. Прошли 52 регрессионных теста и полный stress-тест FTS с checkpoint/reopen.

[Новый отчёт](../benches/results/2026-10-01-bulk-with-appender/report.md). Следующее узкое место — стоимость embedding при вставке. Цель остаётся активной.

</details>

<details>
<summary>25. 2026-09-30T23:17:33.409Z</summary>

Проверю горячий цикл hashing embedder. Попробую ускорить распределение признаков по координатам, сохранив векторы побитно, и оставлю изменение только при измеримом выигрыше.

</details>

<details>
<summary>26. 2026-09-30T23:20:30.849Z</summary>

Проверка распределения координат проходит на крайних и случайных значениях. Дополнительно сравню векторы с исходным алгоритмом побитно. Первый bulk-замер сильно разошёлся с предыдущими даже в вариантах без embedding, поэтому пока не буду приписывать изменение скорости этой оптимизации.

</details>

<details>
<summary>27. 2026-09-30T23:21:59.340Z</summary>

Причина нестабильности замеров стала яснее: сейчас на машине одновременно работают несколько других `rustc`, CAD-тест и виртуальная машина, многие почти полностью занимают ядра. Новые прогоны нельзя считать чистым доказательством ускорения. Проверка векторов на побитное совпадение прошла; отмечу производительность эксперимента как неподтверждённую.

</details>

<details>
<summary>28. 2026-09-30T23:24:18.425Z</summary>

Добавил эксперимент с ускорением hashing embedder: распределение по координатам и векторы побитно совпадают с исходным алгоритмом. Полный набор тестов прошёл, включая stress-тест FTS.

**Ускорение пока не подтверждено:** параллельные сборки и другие процессы сильно исказили замеры. Эксперимент оставлен предварительным; нужен повтор при стабильной нагрузке.

[Отчёт обновлён](benchmark-results-2026-10-01.md). Цель остаётся активной.

</details>

<details>
<summary>29. 2026-09-30T23:24:24.283Z</summary>

Проверю текущую нагрузку и подготовлю сравнение embedding до/после в чередующемся порядке. Это позволит проверить эксперимент без опоры на разошедшиеся общие замеры bulk insert.

</details>

<details>
<summary>30. 2026-09-30T23:31:34.938Z</summary>

Сборка завершилась. Запускаю три пары «исходный modulo / эксперимент» с обратным порядком во второй паре. В каждом прогоне сохраню также контрольные варианты без embedding, чтобы оценить влияние нагрузки.

</details>

<details>
<summary>31. 2026-09-30T23:34:54.958Z</summary>

Три пары завершились. Эксперимент не дал устойчивого выигрыша: медиана bulk 10k — около 22,9 мс против 18,7 мс у исходного варианта, при этом контрольные случаи тоже заметно колебались. Уберу эксперимент и сохраню тест побитной совместимости векторов.

</details>

<details>
<summary>32. 2026-09-30T23:48:43.758Z</summary>

Повторил сравнение в трёх парах процессов с чередованием порядка. Эксперимент с reciprocal **не подтвердил ускорение**, поэтому убрал его и вернул исходный modulo.

Сохранил тест побитной совместимости векторов; все проверки embedding прошли. [Результаты сравнения](../benches/results/2026-10-01-embed-alternating/report.md).

Цель остаётся активной: проигрыши в записи и проверка MSSQL/Kusto ещё не закрыты.

</details>

<details>
<summary>33. 2026-09-30T23:48:49.877Z</summary>

Продолжу с нормализации embedding: проверю, можно ли избежать сортировки занятых координат для небольших значений, сохранив векторы побитно. После проверки корректности повторю замеры вставки.

</details>

<details>
<summary>34. 2026-10-01T00:11:45.768Z</summary>

Во всех трёх парах полная вставка ускорилась. Медиана для 10k строк снизилась с **14,13 до 13,13 мс** — примерно на 7%; контрольные варианты без embedding менялись гораздо меньше. Сохраняю оптимизацию и повторю сравнение с SQLite и DuckDB Appender.

</details>

<details>
<summary>35. 2026-10-01T00:30:47.615Z</summary>

Оптимизация помогла, но bulk insert пока медленнее SQLite и DuckDB Appender. Уберу ещё две лишние операции: повторное выделение памяти при сборке текста для embedding и учёт изменённых строк в пакетах, где нет `cas each`.

</details>

<details>
<summary>36. 2026-10-01T07:03:26.998Z</summary>

Последние изменения прошли тесты. Продолжу с замеров записи: проверю, сократили ли они отставание от SQLite и DuckDB Appender.

</details>

<details>
<summary>37. 2026-10-01T07:04:03.017Z</summary>

В новом прогоне вставка 10 тыс. строк заняла 12,54 мс у Lin, 10,43 мс у SQLite и 10,44 мс у DuckDB Appender. Отставание сохраняется; один прогон не доказывает эффект последних изменений. Также проверю подозрительный случай FTS: несколько смен текста одной строки до слияния отложенных изменений индекса.

</details>

<details>
<summary>38. 2026-10-01T07:04:37.770Z</summary>

Тест подтвердил ошибку FTS: после удаления, возврата и повторного удаления слова индекс снова выдавал старую строку кандидатом. Исправил обработку отложенного удаления; проверяю 600 таких циклов со слияниями индекса и полный набор тестов.

</details>

<details>
<summary>39. 2026-10-01T07:05:15.911Z</summary>

Ускорил нормализацию embedding: три парных прогона показали около 7% выигрыша на вставке 10 тыс. строк. Убрал лишние аллокации и исправил ошибку FTS при повторных изменениях строки. Все тесты прошли.

Цель пока не достигнута: последний замер вставки 10 тыс. строк — Lin 12,54 мс, SQLite 10,43 мс, DuckDB Appender 10,44 мс. MSSQL и Kusto ещё не проверены.

[Результаты и ограничения](benchmark-results-2026-10-01.md).

</details>

<details>
<summary>40. 2026-10-01T07:05:24.493Z</summary>

[Отчёт последнего бенча](../benches/results/2026-10-01-bulk-pack-tracking/report.md).

| Вставка 10 тыс. строк | Медиана |
|---|---:|
| Lin | 12,54 мс |
| SQLite | 10,43 мс |
| DuckDB Appender | 10,44 мс |

Один процесс, 8 замеров. Lin пока медленнее; у него включены embedding и FTS, у SQL — обычная схема.

[Общий отчёт со всеми сравнениями](benchmark-results-2026-10-01.md).

</details>

<details>
<summary>41. 2026-10-01T07:21:02.805Z</summary>

Продолжу оптимизировать вставку: сначала проверю, где в текущем коде тратится время и выделяется память, затем сравню изменение с исходным вариантом.

</details>

<details>
<summary>42. 2026-10-01T07:21:46.813Z</summary>

Нашёл лишнее копирование: для embedding всегда собирается новая строка, даже когда текст есть только в одном поле. Добавлю заимствование текста для этого случая, сохранив прежние пробелы и точные значения векторов.

</details>

<details>
<summary>43. 2026-10-01T07:23:05.414Z</summary>

Убрал копирование текста для embedding, когда используется одно поле. Значения векторов сохранены, тесты прошли.

[Новый замер](../benches/results/2026-10-01-insert-borrowed-text/report.md): 10 тыс. строк — 18,45 мс. Без парного сравнения и при фоновой нагрузке этот результат не доказывает ускорение. Победа над SQLite и DuckDB пока не подтверждена.

</details>

<details>
<summary>44. 2026-10-01T07:23:11.301Z</summary>

Для следующего шага проверю стоимость хеширования при вставке и возможность переиспользовать строковые значения подготовленного запроса. Нужен выигрыш на полной вставке, поэтому буду сравнивать варианты на одинаковых данных.

</details>

<details>
<summary>45. 2026-10-01T07:23:35.250Z</summary>

В аккумуляторе embedding есть отдельный массив `seen`, хотя веса всегда положительные: нулевое значение уже означает, что слот ещё не использовался. Проверю вариант без этого массива — он убирает выделение памяти и дополнительные обращения к ней. Тесты сверят векторы побитно.

</details>

<details>
<summary>46. 2026-10-01T07:24:35.225Z</summary>

Первый парный прогон дал 18,32 → 12,82 мс, но одновременно ускорились контрольные случаи без embedding. Значит, эти 30% нельзя приписать изменению. Повторю сравнение в обратном порядке.

</details>

<details>
<summary>47. 2026-10-01T07:25:12.264Z</summary>

Повтор не подтвердил выигрыш: исходный вариант — 13,18 мс, без `seen` — 13,28 мс. Откатываю эксперимент; результаты сохраню, чтобы не считать его ускорением.

</details>

<details>
<summary>48. 2026-10-01T07:25:37.737Z</summary>

Проверил ещё одну оптимизацию embedding и откатил: повторный замер показал 13,18 мс до изменения и 13,28 мс после. Выигрыша нет; тесты восстановленного кода прошли.

[Парные результаты](../benches/results/2026-10-01-embed-seen-after-repeat/report.md). Цель обогнать все указанные системы остаётся открытой.

</details>

<details>
<summary>49. 2026-10-01T07:25:49.625Z</summary>

Проверю сборку строк при вставке: сейчас поля добавляются в `BTreeMap` по одному. Попробую массовую сборку и сравню варианты, включая корректность обработки повторяющихся полей.

</details>

<details>
<summary>50. 2026-10-01T07:27:06.561Z</summary>

Массовая сборка прошла полный набор тестов. Отдельная проверка подтвердила, что повторяющееся поле сохраняет последнее значение, а `now` вычисляется при каждом выполнении. Теперь проверяю эффект на вставке 10 тыс. строк.

</details>

<details>
<summary>51. 2026-10-01T07:27:36.026Z</summary>

Массовая сборка оказалась медленнее во всех четырёх случаях: полная вставка — 12,48 → 13,37 мс, без embedding и индексов — 5,69 → 6,44 мс. Откатываю её. Эти замеры указывают, что для небольших строк поэлементная сборка дешевле.

</details>

<details>
<summary>52. 2026-10-01T07:28:04.692Z</summary>

Проверил массовую сборку строк и откатил: вставка 10 тыс. строк ухудшилась с 12,48 до 13,37 мс. Все четыре измеренных случая стали медленнее.

[Сравнение вариантов](../benches/results/2026-10-01-row-build-after/report.md). Тесты прошли; подтверждённого ускорения в этом эксперименте нет.

</details>

<details>
<summary>53. 2026-10-01T07:28:08.707Z</summary>

Два последних эксперимента не дали выигрыша. Сниму профиль полной вставки, чтобы выбрать следующий участок по затратам CPU.

</details>

<details>
<summary>54. 2026-10-01T07:29:23.036Z</summary>

В профиле выполнения заметная доля выборок приходится на сборку строк и выделение памяти. Подготовленный запрос при каждом запуске заново превращает строковые литералы в `Arc<str>`. Попробую хранить готовые строки в `Prepared` для вставок без `now`; ID, хеши, embedding и проверки останутся на этапе выполнения.

</details>

<details>
<summary>55. 2026-10-01T07:31:51.737Z</summary>

Проверки нового пути прошли, в том числе повторный запуск подготовленной вставки с новыми ID и обработка `now` без шаблона. В первой паре выигрыш полной вставки небольшой — 13,00 → 12,77 мс, а часть остальных случаев ухудшилась. Дождусь ещё двух пар, прежде чем решать, оставлять ли изменение.

</details>

<details>
<summary>56. 2026-10-01T07:32:23.066Z</summary>

Три пары не подтвердили выигрыш: медиана полной вставки — 13,32 мс у исходного варианта и 13,37 мс у шаблонов. В последней паре шаблоны также проиграли. Откатываю изменение: дополнительная память в `Prepared` пока не оправдана замерами.

</details>

<details>
<summary>57. 2026-10-01T07:32:59.986Z</summary>

Снял профиль вставки и проверил шаблоны строк в `Prepared`. Три парных прогона не подтвердили ускорение: 13,32 мс исходный вариант, 13,37 мс с шаблонами. Изменение откатил; проверка восстановленного кода прошла.

[Результаты трёх пар](../benches/results/2026-10-01-insert-templates-ab/report.md). Цель пока не достигнута.

</details>

<details>
<summary>58. 2026-10-01T07:33:06.879Z</summary>

Проверю скалярные индексы при массовой вставке. По предыдущим замерам они дают заметную часть стоимости; посмотрю, можно ли обрабатывать пакет целиком вместо обновления дерева для каждой строки.

</details>

<details>
<summary>59. 2026-10-01T07:34:38.992Z</summary>

Добавил вариант для больших вставок в пустой индекс: сначала сортировать ключи, затем строить дерево целиком. Для заполненного индекса и небольших пакетов оставил прежний путь. Проверю совпадение индексов после обновлений и удалений, а также ошибки уникальности.

</details>

<details>
<summary>60. 2026-10-01T07:36:05.548Z</summary>

Проверки индекса прошли, включая последующие удаления и вставки. Первые замеры пока не в пользу пакетной сортировки: дополнительная сортировка может съедать выигрыш от построения дерева. Дождусь всех трёх пар и оставлю вариант только при подтверждённом улучшении.

</details>

<details>
<summary>61. 2026-10-01T07:36:31.056Z</summary>

Во всех трёх парах пакетная сортировка проиграла. Медианы по процессам: полная вставка — 12,89 мс исходный вариант против 13,83 мс нового; без embedding — 9,09 против 9,87 мс. Откатываю её.

</details>

<details>
<summary>62. 2026-10-01T07:37:01.899Z</summary>

Пакетная сборка индекса проиграла во всех трёх парных прогонах: 12,89 мс исходный вариант против 13,83 мс нового. Откатил изменение; 52 теста выполнения прошли.

[Результаты сравнения](../benches/results/2026-10-01-bulk-tree-ab/report.md). Подтверждённого ускорения нет, цель остаётся открытой.

</details>

<details>
<summary>63. 2026-10-01T07:37:09.927Z</summary>

Следующий участок — embedding: проверю копирование готового вектора при передаче в `Arc`. Возможно, здесь удастся убрать полную копию, сохранив переиспользование аккумулятора и точные значения.

</details>

<details>
<summary>64. 2026-10-01T07:38:11.332Z</summary>

Копирование в `Arc` связано с переиспользованием плотного аккумулятора; простого выигрыша здесь не вижу. Но нашёл более перспективный путь в FTS: для новых последовательных строк сейчас используется общий механизм с сортировкой токенов и проверкой отложенных изменений. Добавлю прямое дописывание, когда таких изменений нет, с возвратом к общему пути после удалений.

</details>

<details>
<summary>65. 2026-10-01T07:40:01.241Z</summary>

Новый FTS-путь прошёл полный набор тестов. Отдельный тест подтвердил совпадение со старым механизмом после удаления и повторного использования хвостовой позиции, включая Unicode и повторяющиеся слова. Запустил три парных сравнения с чередованием порядка вариантов.

</details>

<details>
<summary>66. 2026-10-01T07:41:27.389Z</summary>

Прямое дописывание FTS выиграло во всех трёх парах полной вставки: примерно 3,2%, 8,5% и 4,5%. Оставляю изменение. Теперь сравню текущий код с SQLite и DuckDB Appender; выигрыша над прежним Lin недостаточно для общей цели.

</details>

<details>
<summary>67. 2026-10-01T07:42:59.840Z</summary>

Оставил ускорение FTS: три парных прогона показали выигрыш полной вставки, медиана — **4,5%**. Тесты прошли.

Но в новом сравнении 10 тыс. строк: Lin **14,64 мс**, SQLite **10,62 мс**, DuckDB Appender **10,48 мс**. Отставание сохраняется.

[Парные замеры](../benches/results/2026-10-01-fts-append-ab/report.md) · [Сравнение с конкурентами](../benches/results/2026-10-01-bulk-fts-append/report.md).

</details>

<details>
<summary>68. 2026-10-01T07:43:06.078Z</summary>

Проверю актуальные сравнения чтения с DuckDB, SQLite и pandas на 100 тыс. строк. Это обновит общую картину после изменений; отставание на вставке остаётся отдельной задачей.

</details>

<details>
<summary>69. 2026-10-01T07:43:55.481Z</summary>

Для MSSQL и Kusto в окружении по-прежнему нет адресов тестовых серверов. Запущу сравнение со всеми восемью заявленными системами: для остальных шести подниму отдельные тестовые серверы, а отсутствие двух подключений отчёт отметит как неполное покрытие.

</details>

<details>
<summary>70. 2026-10-01T07:45:02.913Z</summary>

В первом процессе Lin быстрее DuckDB, SQLite, PostgreSQL и MySQL во всех шести проверенных запросах; MongoDB ещё измеряется. В частности, поиск подстроки — около 208 мкс у Lin против 535 мкс у DuckDB. Два следующих процесса проверят повторяемость результатов.

</details>

<details>
<summary>71. 2026-10-01T07:46:13.196Z</summary>

Первый процесс завершился: Lin быстрее всех шести доступных конкурентов во всех шести запросах. Во втором процессе пока сохраняются выигрыши, но времена заметно колеблются. Итог посчитаю по медианам отдельных процессов; результаты относятся к API и включают передачу данных от серверов.

</details>

<details>
<summary>72. 2026-10-01T07:47:49.946Z</summary>

Второй процесс завершён, третий сейчас готовит данные. Тесты результатов проверяют точные значения и повторяющиеся строки вне измеряемого времени. Даже если чтение выиграет во всех доступных сравнениях, общая цель останется открытой из-за вставки и отсутствующих MSSQL/Kusto.

</details>

<details>
<summary>73. 2026-10-01T07:49:15.630Z</summary>

Три процесса завершены: по итоговым медианам Lin быстрее во всех 36 измеренных сравнениях чтения. Проверка полного покрытия вернула код 2, поскольку MSSQL и Kusto недоступны. Вставка по последним замерам всё ещё проигрывает SQLite и DuckDB Appender.

</details>

<details>
<summary>74. 2026-10-01T07:50:02.436Z</summary>

Свежий бенч: **100 тыс. строк, 3 независимых процесса**. Lin выиграл **36 из 36 измеренных сравнений чтения** с DuckDB, SQLite, PostgreSQL, MySQL, MongoDB и pandas.

[Краткая таблица](../benches/results/2026-10-01-peer-current-100k/summary.md) · [Полный отчёт](../benches/results/2026-10-01-peer-current-100k/report.md).

Это сравнение API, включая передачу данных от серверов. MSSQL/Kusto ещё не проверены; вставка по-прежнему медленнее SQLite и DuckDB Appender.

</details>

<details>
<summary>75. 2026-10-01T07:50:09.814Z</summary>

Продолжу с точечными изменениями: проверю удаление одной строки, где Lin ранее отставал от SQLite. Посмотрю, какие данные копируются после поиска строки по ID.

</details>

<details>
<summary>76. 2026-10-01T07:54:51.014Z</summary>

Первый прогон неоднозначен: в таблице 10 тыс. строк удаление стало быстрее, а на других размерах — медленнее; времена SQLite тоже изменились. Увеличил число свежих измерений до 40 и запустил три парных сравнения с чередованием порядка.

</details>

<details>
<summary>77. 2026-10-01T07:56:39.170Z</summary>

Первая пара с 40 измерениями дала 22,08 → 18,98 мкс на таблице 100 тыс. строк, но SQLite в этой паре ускорилась примерно в той же пропорции. На 1 тыс. строк новый вариант проиграл. Пока устойчивый эффект изменения не подтверждён.

</details>

<details>
<summary>78. 2026-10-01T07:58:49.205Z</summary>

Вторая пара тоже дала смешанный результат: удаление на 1 тыс. и 10 тыс. строк ускорилось, на 100 тыс. — немного замедлилось. Последний прогон ещё идёт; решение приму по всей матрице размеров.

</details>

<details>
<summary>79. 2026-10-01T08:00:34.940Z</summary>

Три пары не подтвердили устойчивое ускорение. Последняя дала замедление на всех размерах; на 100 тыс. строк — 23,48 → 27,04 мкс, хотя SQLite там немного ускорилась. Откатываю изменение. Тест порядка удалённых строк и отката оставлю.

</details>

<details>
<summary>80. 2026-10-01T08:01:14.838Z</summary>

Проверил удаление без лишней копии: три парных прогона по 40 измерений не подтвердили устойчивого ускорения. Изменение откатил, тест порядка строк и отката оставил. Все 53 теста выполнения прошли.

[Замеры на трёх размерах таблиц](../benches/results/2026-10-01-delete-owned-result-ab/report.md). Цель пока не достигнута.

</details>

<details>
<summary>81. 2026-10-01T08:01:22.548Z</summary>

Продолжу с измерения стоимости embedding при вставке. Хочу проверить, можно ли сократить повторное хеширование коротких фрагментов текста, сохранив векторы побитно.

</details>

<details>
<summary>82. 2026-10-01T08:04:49.799Z</summary>

Побитная проверка прошла для всех 65 536 пар байтов, нескольких размерностей и повторного использования таблицы. Новый путь также совпал с одиночным embedding на пакете с Unicode и смешанным регистром. Полный набор тестов ещё выполняет проверку удаления с checkpoint/reopen.

</details>

<details>
<summary>83. 2026-10-01T08:06:32.615Z</summary>

Полный набор тестов прошёл. Таблица слотов занимает дополнительно 256 КиБ только в пакетах от 1024 текстов и освобождается после вызова. В одиночных запросах и небольших пакетах её нет. Запустил парные замеры полной вставки, чтобы проверить, окупается ли эта память временем.

</details>

<details>
<summary>84. 2026-10-01T08:08:52.213Z</summary>

Таблица слотов проиграла во всех трёх парах полной вставки: 24,43 → 30,08 мс, 12,26 → 15,42 мс и 14,32 → 16,19 мс. Фоновая нагрузка колебалась, но оснований оставлять дополнительную память нет. Откатываю эксперимент.

</details>

<details>
<summary>85. 2026-10-01T08:09:26.653Z</summary>

Проверил кеш слотов для embedding: он проиграл во всех трёх парных прогонах вставки. Откатил эксперимент; тесты восстановленного кода прошли.

[Замеры и решение](../benches/results/2026-10-01-pair-slots-ab/report.md). Ускорение не подтверждено, цель остаётся открытой.

</details>

<details>
<summary>86. 2026-10-01T08:11:15.775Z</summary>

Проверю более компактную подготовку вставки. Прошлый вариант хранил готовые строки целиком и не ускорился. Теперь попробую сохранять только значения ячеек, чтобы при повторном запуске не выделять память для строковых литералов. Поля `now` останутся вычисляемыми при выполнении.

</details>

<details>
<summary>87. 2026-10-01T08:16:01.162Z</summary>

Полный набор тестов прошёл. Отдельная проверка подтвердила корректность двух вставок в одном подготовленном пакете, повторного запуска клона, новых ID и времени выполнения. Парные замеры теперь проверяют, окупается ли дополнительная память под значения ячеек.

</details>

<details>
<summary>88. 2026-10-01T08:18:03.542Z</summary>

Подготовленные значения не дали устойчивого выигрыша: две пары немного ускорились, одна заметно замедлилась. Откатываю этот вариант. При проверке пути вставки заметил потенциальную ошибку: большие пакеты не возвращают строки в `Handle`, но запись WAL, похоже, строится именно из этого результата. Проверю сохранность после переоткрытия.

</details>

<details>
<summary>89. 2026-10-01T08:21:52.676Z</summary>

Тест подтвердил ошибку: вставка 129 документов создавала строки в памяти, но экспортированный WAL передавал реплике ноль строк. Исправлю запись WAL, чтобы она читала вставленный участок хранилища, и проверю счётчик `done.n` на границе 128/129 строк.

</details>

<details>
<summary>90. 2026-10-01T08:25:41.669Z</summary>

Проверка WAL теперь проходит: реплика получает все 129 строк вместе с полями и embedding. Добавлю в бенч большой вставки проверку `done.n` и чтение результата вне таймера, чтобы измерение не могло пройти при пустом результате операции.

</details>

<details>
<summary>91. 2026-10-01T08:29:01.764Z</summary>

Полный набор тестов прошёл после исправления WAL и `done.n`, включая границу 128/129 строк. Бенч теперь проверяет число вставленных строк, их статические поля, тип timestamp и наличие embedding вне измеряемого участка. Запускаю его с SQLite и DuckDB Appender.

</details>

<details>
<summary>92. 2026-10-01T08:30:56.744Z</summary>

Исправил потерю строк в WAL при вставке **более 128 документов** и неверный `done.n`. Проверки репликации, переоткрытия и бенч с чтением результата прошли.

Последний замер 10 тыс. строк: Lin **12,76 мс**, SQLite **10,59 мс**, DuckDB Appender **10,21 мс**. Цель пока не достигнута.

[Отчёт бенча](../benches/results/2026-10-01-bulk-wal-count-fixed/report.md).

</details>

<details>
<summary>93. 2026-10-01T08:32:11.408Z</summary>

Попробую распараллелить embedding в больших пакетах: это заметная часть стоимости вставки. Проверю вариант с несколькими потоками, сохранив порядок результатов и точные значения векторов. Затем сравню полную вставку с прежним вариантом и SQL-конкурентами.

</details>

<details>
<summary>94. 2026-10-01T08:37:10.341Z</summary>

Параллельный путь сохранил порядок и побитное совпадение с одиночным embedding на границе 4096 текстов. Код использует до четырёх потоков и отдельный аккумулятор в каждом. Малые пакеты и пользовательские реализации `Embedder` выполняются как раньше.

</details>

<details>
<summary>95. 2026-10-01T08:40:54.296Z</summary>

Создание новых потоков на каждый пакет не дало устойчивого выигрыша: одна пара ускорилась, две замедлились. Проверю вариант с переиспользуемым пулом потоков — он уберёт повторное создание потоков из каждой вставки.

</details>

<details>
<summary>96. 2026-10-01T08:46:49.148Z</summary>

Замеры варианта с переиспользуемым пулом завершились. Сравню три пары запусков и контрольные вставки без эмбеддингов, чтобы проверить, устойчиво ли ускорение.

</details>

<details>
<summary>97. 2026-10-01T08:47:41.102Z</summary>

Пул пока не оставляю: устойчивого выигрыша нет, а контрольные замеры сильно колеблются — от 9 до 79 мс. Сохраню результаты и верну последовательный вариант. Подтверждённые ускорения FTS и нормализации эмбеддингов останутся.

</details>

<details>
<summary>98. 2026-10-01T08:48:53.621Z</summary>

Проверки эмбеддингов после отката прошли: векторы побитно совпадают с исходным алгоритмом. Завершаю повторный бенч вставок на 1 000 и 10 000 строк с проверкой записанных данных; результаты сохраню в отдельный отчёт.

</details>

<details>
<summary>99. 2026-10-01T08:49:39.575Z</summary>

Оставил подтверждённые оптимизации: FTS дал около **4,5%**, нормализация эмбеддингов — около **7%** ускорения вставки в парных замерах. Параллельный пул откатил: устойчивого выигрыша нет.

[Свежий результат бенча](../benches/results/2026-10-01-serial-final-bulk/report.md):

| Вставка | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1 000 строк | 1,759 мс | 1,110 мс | 1,501 мс |
| 10 000 строк | 15,738 мс | 13,927 мс | 12,193 мс |

На массовой вставке **пока проигрываем**. Проверка записанных данных, побитная совместимость эмбеддингов и восстановление большого insert через WAL прошли.

</details>

<details>
<summary>100. 2026-10-01T08:49:44.235Z</summary>

Продолжу с массовой вставки: найду следующий источник затрат и проверю изменение парными замерами.

</details>

<details>
<summary>101. 2026-10-01T08:53:01.650Z</summary>

Проверки нового формата ключей прошли: порядок сортировки, уникальность, обновление и удаление сохраняются для ключей из 1–5 полей. Первая пара бенчей быстрее, но ускорились и контрольные случаи без индекса; дождусь остальных пар, прежде чем делать вывод.

</details>

<details>
<summary>102. 2026-10-01T08:54:34.942Z</summary>

Короткие ключи выиграли все три пары по полной вставке: примерно 6–11%. Проверю ещё раз, заранее собрав оба бинарника — так компиляция между замерами не будет влиять на нагрузку. Также проверю, не замедлилось ли чтение индекса.

</details>

<details>
<summary>103. 2026-10-01T08:57:28.557Z</summary>

В шести парах заранее собранных бинарников медианный выигрыш по полной вставке — 3,4%; четыре пары выиграны, две проиграны. Случаи без скалярного индекса такого эффекта не показали. Нашёл ещё три временных буфера при построении границ поиска — переведу короткие границы на тот же формат и повторю проверку чтения.

</details>

<details>
<summary>104. 2026-10-01T08:59:31.463Z</summary>

Поиск по равенству ускорился во всех шести парах; диапазонный поиск — в пяти из шести. Полный набор тестов прошёл, включая откаты, уникальные индексы, WAL и восстановление. Остался финальный замер массовой вставки против SQLite и DuckDB Appender.

</details>

<details>
<summary>105. 2026-10-01T09:00:47.417Z</summary>

Убрал heap-выделения для коротких ключей индекса и границ поиска.

Парные замеры показали:

- Поиск по равенству: **+10,6%**, выиграны 6/6 пар.
- Диапазонный поиск: **+4,3%**, выиграны 5/6.
- Вставка 10 000 строк: **+3,4%**, выиграны 4/6.

[Отчёт и сырые результаты](../benches/results/2026-10-01-inline-index-ab/report.md). Полный набор тестов прошёл.

Общая цель ещё не достигнута: массовая вставка проигрывает SQLite и DuckDB Appender; сравнения с MSSQL/Kusto пока отсутствуют.

</details>

<details>
<summary>106. 2026-10-01T09:00:52.880Z</summary>

Продолжу ускорять вставку. Проверю предварительное резервирование обратной карты индекса: сейчас она растёт по мере добавления строк и может несколько раз перераспределять память.

</details>

<details>
<summary>107. 2026-10-01T09:06:08.901Z</summary>

Новый путь удаления прошёл полный набор тестов. Однако в первых двух парах он замедлил удаление из 100 000 строк; SQLite в этих контрольных замерах почти не изменился. Дождусь третьей пары и откатю вариант, если результат подтвердится.

</details>

<details>
<summary>108. 2026-10-01T09:06:55.406Z</summary>

Проверил ещё два варианта и откатил оба:

- Резервирование карты индекса: проиграло 4 из 6 пар.
- Перенос позиции без пересоздания ключа: ускорил удаление при 10 000 строк, но замедлил при 1 000; при 100 000 результат неустойчивый.

[Отчёт удаления](../benches/results/2026-10-01-index-move-ab/report.md). Тесты прошли; предыдущие подтверждённые оптимизации сохранены. Цель обогнать всех конкурентов остаётся открытой.

</details>

<details>
<summary>109. 2026-10-01T09:08:53.280Z</summary>

Полный набор тестов прошёл, включая удаления, откаты и восстановление. Теперь сравниваю два заранее собранных бинарника на 1 000, 10 000 и 100 000 строках: три пары запусков, по 24 свежих состояния на случай. SQLite остаётся контролем в каждом запуске.

</details>

<details>
<summary>110. 2026-10-01T09:11:04.027Z</summary>

Выигрыш снова оказался неустойчивым: на 100 000 строках выиграны две пары из трёх, на 10 000 проиграны все три. Откатю изменение и сохраню таблицу замеров. SQLite пока остаётся быстрее Lin во всех проверенных размерах.

</details>

<details>
<summary>111. 2026-10-01T09:11:36.143Z</summary>

Проверил обновление позиций в картах ID/URI без пересоздания ключей. Тесты прошли, но устойчивого ускорения нет: при 10 000 строках проиграны все три пары. Изменение откатил.

[Таблица замеров](../benches/results/2026-10-01-map-position-ab/report.md). Предыдущие подтверждённые оптимизации сохранены. SQLite по удалению пока быстрее.

</details>

<details>
<summary>112. 2026-10-01T09:11:42.351Z</summary>

Сменю подход: вместо следующей гипотезы сниму профиль повторяемого удаления, чтобы найти затраты, которые действительно остаются в горячем пути.

</details>

<details>
<summary>113. 2026-10-01T09:14:16.139Z</summary>

Профиль показал заметные затраты FTS: среди частых функций оказались слияние и вычитание списков позиций, а также перенос и удаление строки. Диагностический прогон завершился, проверки количества строк и lookup по ID/URI прошли. Это другой сценарий, чем свежий одиночный delete в бенче; сохраню профиль как основание для следующего эксперимента с FTS.

</details>

<details>
<summary>114. 2026-10-01T09:14:47.827Z</summary>

Снял профиль повторяемого удаления и вставки. Заметные затраты обнаружились в FTS: слияние, вычитание и перенос списков позиций.

[Профиль и ограничения замера](../benches/results/2026-10-01-prepared-write-profile/report.md). Прогон завершился, проверки строк и lookup прошли.

Следующий эксперимент — уменьшить работу FTS при удалении. Ускорение и победа над конкурентами пока не доказаны.

</details>

<details>
<summary>115. 2026-10-01T09:14:53.927Z</summary>

Разберу обновление списков FTS при удалении и переносе строки. Изменение проверю на повторных правках, откатах и восстановлении, затем — на свежих удалениях в бенче.

</details>

<details>
<summary>116. 2026-10-01T09:17:56.544Z</summary>

Нашёл ошибку корректности: слияние FTS ожидает отсортированные списки правок, но добавление и отмена правок нарушали порядок. Регрессионный тест воспроизвёл оставшуюся позицию после удалений в порядке 8 → 2, включая исходный код до эксперимента. Исправил сохранение порядка; оптимизацию слияния отложил до проверки корректной базы.

</details>

<details>
<summary>117. 2026-10-01T09:20:45.715Z</summary>

Исправил ошибку FTS: правки нарушали порядок списков, из-за чего после удалений 8 → 2 индекс сохранял удалённую позицию 2.

Регрессионные тесты, 3 000 случайных правок, восстановление индекса и полный набор тестов прошли.

[Отчёт и свежий бенч](../benches/results/2026-10-01-fts-sorted-pending/report.md). SQLite по удалению пока быстрее; ускорение слияния FTS ещё не подтверждено.

</details>

<details>
<summary>118. 2026-10-01T09:21:17.875Z</summary>

Теперь проверю свёртку FTS на корректной базе. Попробую удалять позиции из существующего буфера и сохранять строковый ключ термина, чтобы убрать лишние выделения памяти при свёртке.

</details>

<details>
<summary>119. 2026-10-01T09:24:26.406Z</summary>

Полный набор тестов прошёл, включая случайные правки FTS и сохранение/восстановление. Запустил три пары прогонов на заранее собранных бинарниках. После каждого прогона проверяются ID, URI и точный набор результатов лексического поиска.

</details>

<details>
<summary>120. 2026-10-01T09:26:35.933Z</summary>

Свёртка FTS в существующем буфере не дала устойчивого ускорения: две пары проиграны, одна выиграна. Вариант откатил; исправление порядка правок сохранено.

[Парные замеры](../benches/results/2026-10-01-fts-inplace-fold-ab/report.md). Все прогоны прошли точную проверку результатов поиска. Победа над всеми конкурентами пока не достигнута.

</details>

<details>
<summary>121. 2026-10-01T09:26:43.172Z</summary>

Проверю ещё один вариант: объединить вычитание удалённых позиций и добавление новых в один проход. Сравню его с исправленной базой FTS, сохранив проверки результатов поиска.

</details>

<details>
<summary>122. 2026-10-01T09:28:37.749Z</summary>

Новый вариант прошёл полный набор тестов, включая 32 768 сочетаний маленьких списков, случайные правки и восстановление FTS. Идут парные замеры повторяемых удалений и вставок; оба бинарника после каждого прогона проверяют точный набор результатов поиска.

</details>

<details>
<summary>123. 2026-10-01T09:29:47.451Z</summary>

Один проход выиграл все три пары по пропускной способности: медианное улучшение — около 12,5%, хотя во второй паре разница почти нулевая. Проверки поиска прошли. Оставлю вариант и проверю свежие одиночные удаления и массовые вставки против SQLite и DuckDB Appender.

</details>

<details>
<summary>124. 2026-10-01T09:32:03.018Z</summary>

Оставил слияние FTS за один проход: медианный прирост пропускной способности — **12,5%** в повторяемых удалениях/вставках. Выиграны 3/3 пары, одна почти вничью. Тесты и точные проверки поиска прошли.

[Отчёт и свежие сравнения](../benches/results/2026-10-01-fts-fused-correct-ab/report.md).

Цель пока не достигнута: одиночное удаление и вставка 10 000 строк ещё проигрывают SQLite и DuckDB Appender.

</details>

<details>
<summary>125. 2026-10-01T09:32:09.075Z</summary>

Вернусь к массовой вставке. Проверю, сколько времени уходит на хеширование содержимого, и можно ли ускорить его без изменения сохраняемых хешей.

</details>

<details>
<summary>126. 2026-10-01T09:35:11.409Z</summary>

Хеш содержимого не выделился среди главных затрат профиля. Зато видны Unicode-преобразования регистра даже для ASCII-текста. Добавлю отдельный ASCII-путь; Unicode оставлю на прежнем алгоритме и проверю совпадение текстов и эмбеддингов.

</details>

<details>
<summary>127. 2026-10-01T09:37:12.623Z</summary>

Полный набор тестов прошёл. Новый путь совпадает с прежним преобразованием для всех ASCII-символов и смешанного Unicode; проверки эмбеддингов тоже прошли. Завершаю сборку второго бинарника для парных замеров вставки.

</details>

<details>
<summary>128. 2026-10-01T09:43:27.209Z</summary>

ASCII-путь выиграл все три пары по полной вставке: время меньше на 6,8%, 9,0% и 7,6%. Контрольные случаи тоже колебались, поэтому весь выигрыш нельзя уверенно приписать изменению. Оставлю вариант; полный набор тестов на текущем коде прошёл.

Свежая вставка 10 000 строк: Lin 11,67 мс, SQLite 10,86 мс, DuckDB Appender 10,41 мс. Отставание ещё остаётся.

</details>

<details>
<summary>129. 2026-10-01T09:44:28.518Z</summary>

Оставил ASCII-путь преобразования регистра. Все три пары вставки быстрее; медианное сокращение времени — **7,6%**, но часть разницы могла дать фоновая нагрузка. Тесты совместимости прошли.

[Отчёт и сырые замеры](../benches/results/2026-10-01-ascii-lower-ab/report.md).

Свежая вставка 10 000 строк: **Lin 11,67 мс**, SQLite 10,86 мс, DuckDB Appender 10,41 мс. Цель ещё не достигнута.

</details>

<details>
<summary>130. 2026-10-01T09:44:36.000Z</summary>

Проверю следующий источник затрат вставки: сравнений строковых ключей в карте полей много в профиле. Сначала посмотрю, можно ли сократить их без изменения публичного формата строк и поведения дубликатов полей.

</details>

<details>
<summary>131. 2026-10-01T09:55:43.231Z</summary>

`src/exec.rs` снова изменился во время сборки. Для этого сравнения создам изолированный снимок текущих исходников и соберу оба варианта в нём. Так параллельная работа не будет менять код между замерами.

</details>

<details>
<summary>132. 2026-10-01T09:58:34.713Z</summary>

Вариант с общими строками ID/URI прошёл полный набор тестов, включая удаление, откаты и checkpoint/reopen. Изолированные бинарники собраны; идут замеры вставки и точечного чтения, затем сравнение со SQLite и DuckDB Appender.

</details>

<details>
<summary>133. 2026-10-01T10:00:24.837Z</summary>

Первые три пары дают выигрыш вставки в двух случаях, но точечное чтение немного колеблется. Добавлю ещё три пары на тех же бинарниках, чтобы оценить оба эффекта перед решением.

</details>

<details>
<summary>134. 2026-10-01T10:03:02.376Z</summary>

По шести парам медианное сокращение времени вставки — около 8%; выиграны четыре пары. Точечное чтение в среднем осталось практически на прежнем уровне. Оставлю общие строки ID/URI и проверю текущую рабочую версию тестами и бенчем одиночных записей.

</details>

<details>
<summary>135. 2026-10-01T10:04:52.217Z</summary>

Тесты текущей рабочей версии прошли. Первый бенч удаления я запустил одновременно с ними, поэтому его тайминги исключаю и повторю замер отдельно. Основные парные результаты вставки и чтения были получены без такого пересечения.

</details>

<details>
<summary>136. 2026-10-01T10:08:06.719Z</summary>

Свежий бенч удаления всё ещё показывает отставание от SQLite. Чтобы не оценивать влияние общих строк по разным одиночным запускам, сравню обновление и удаление попарно с исходным вариантом.

</details>

<details>
<summary>137. 2026-10-01T10:49:27.719Z</summary>

Оптимизации и парные замеры готовы. Проверяю итоговый отчёт: вставка ускорилась в большинстве прогонов, но обновления дали смешанный результат — этот компромисс явно укажу в результатах.

</details>

<details>
<summary>138. 2026-10-01T10:49:39.755Z</summary>

Оптимизировал индексы, FTS и обработку текста. Тесты прошли.

Парные бенчи показывают:

- Вставка: медианное сокращение времени **8%**, выигрыш в 4 из 6 пар.
- FTS удаление/повторная вставка: медианное ускорение **12,5%**.
- Точечное чтение после изменения ключей ID/URI практически без изменений.

Есть нерешённый компромисс: обновление на 100 000 строках замедлилось в 2 из 3 пар. Вставка 10 000 строк и удаление пока проигрывают SQLite; вставка также проигрывает DuckDB Appender.

[Полный отчёт и результаты замеров](../benches/results/2026-10-01-shared-identity-ab/report.md). Цель обогнать все перечисленные системы пока не достигнута.

</details>

<details>
<summary>139. 2026-10-01T10:49:45.531Z</summary>

Продолжу с удаления: оно всё ещё проигрывает SQLite. Проверю текущий путь обновления индексов и выберу изменение для парного замера.

</details>

<details>
<summary>140. 2026-10-01T10:50:37.552Z</summary>

В FTS удаление сначала собирает и сортирует токены строки. Проверю обработку токенов по мере чтения, чтобы убрать временный вектор. Сохраню прежние правила Unicode и отдельно проверю повторяющиеся слова.

</details>

<details>
<summary>141. 2026-10-01T10:52:20.986Z</summary>

Потоковый вариант прошёл девять FTS-тестов, включая повторные слова и удаления через свёртку индекса. Теперь сравню его с исходным вариантом на готовых бинарниках; сборка и тесты завершатся до замеров.

</details>

<details>
<summary>142. 2026-10-01T10:54:56.881Z</summary>

Проверил удаление без временного вектора токенов FTS. Тесты прошли, но в трёх парных замерах вариант проиграл текущему на 10 000 и 100 000 строках во всех парах. Его не применял.

[Результаты и исходные замеры](../benches/results/2026-10-01-stream-delete-ab/report.md).

Удаление всё ещё проигрывает SQLite; цель обогнать все перечисленные системы остаётся открытой.

</details>

<details>
<summary>143. 2026-10-01T10:55:13.661Z</summary>

Предыдущий эксперимент исключил потоковую обработку FTS как способ ускорить удаление. Теперь проверю скалярный индекс: обратная карта хранит числовые позиции строк в хеш-таблице, хотя строки в хранилище лежат в плотном массиве. Сравню её с массивом ключей и проверю влияние на вставку и чтение.

</details>

<details>
<summary>144. 2026-10-01T10:57:09.331Z</summary>

Вариант с массивом обратных ключей уже проходит общие тесты удаления и восстановления. Основной код пока не меняю: сначала измерю обновление, удаление, пакетную вставку и чтение индекса на одинаковых снимках исходников.

</details>

<details>
<summary>145. 2026-10-01T10:59:58.979Z</summary>

Полный набор тестов прошёл. Дополнительная проверка на 3 000 случайных вставках, обновлениях и удалениях тоже совпала с эталонной моделью, включая разреженные позиции и клонирование индекса. Идут три пары замеров с чередованием порядка запуска.

</details>

<details>
<summary>146. 2026-10-01T11:01:17.940Z</summary>

Первая пара дала смешанный результат: удаление на 10 000 строках ускорилось, а операции на 100 000 — замедлились. На машине также идут сторонние сборки, поэтому сохраню данные о фоновой нагрузке и проверю, повторится ли результат в остальных парах.

</details>

<details>
<summary>147. 2026-10-01T11:03:45.539Z</summary>

Проверил массив обратных ключей вместо хеш-таблицы. В трёх парных замерах обновление и удаление на 100 000 строках проиграли текущей реализации во всех парах. Изменение не применял.

Полный набор тестов и проверка на 3 000 случайных операций прошли. [Отчёт и замеры](../benches/results/2026-10-01-dense-reverse-ab/report.md).

Устойчивое превосходство над всеми восемью системами пока не подтверждено.

</details>

<details>
<summary>148. 2026-10-01T11:05:51.366Z</summary>

Здесь можно обходить индексы напрямую: функция не меняет саму карту индексов. Порядок обхода и обработка ошибки уникальности сохранятся. Базовый бинарник ещё собирается; замеры начну после завершения обеих сборок и тестов.

</details>

<details>
<summary>149. 2026-10-01T11:07:53.668Z</summary>

Прямой обход индексов прошёл полный набор тестов, включая откаты и чтение после удаления. Осталось собрать вариант для замеров; изменение пока только в изолированном снимке.

</details>

<details>
<summary>150. 2026-10-01T11:09:48.219Z</summary>

В первой паре обновление ускорилось на всех трёх размерах, но удаление на 100 000 строках замедлилось. Продолжаю замеры: одного запуска недостаточно, чтобы выбрать этот вариант.

</details>

<details>
<summary>151. 2026-10-01T11:11:27.243Z</summary>

Во второй паре обновление снова быстрее на всех трёх размерах; удаление остаётся нестабильным. На 100 000 строках новый вариант обновления быстрее SQLite в обеих завершённых парах. Это пока результат конкретного сценария, а не подтверждение общей цели.

</details>

<details>
<summary>152. 2026-10-01T11:14:11.428Z</summary>

Проверил ещё один пробел в сравнении: локальные варианты MSSQL и Kusto не поддерживают ARM этой машины. Для замеров этих двух систем потребуется подходящая внешняя среда; Microsoft также указывает, что производительность эмулятора Kusto отличается от облачного сервиса. [MSSQL](https://learn.microsoft.com/en-us/sql/linux/quickstart-install-connect-docker?view=sql-server-ver17), [Kusto](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install), [производительность эмулятора](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview).

</details>

<details>
<summary>153. 2026-10-01T11:16:20.911Z</summary>

Шесть пар завершены. Обновление на 100 000 строках выиграло 5 из 6 сравнений с текущим кодом; медианное сокращение времени — 2,3%. Удаление выиграло лишь половину пар, поэтому его ускорение не подтверждено. Сохраню прямой обход индексов: он убирает временные выделения памяти и дал положительный результат для обновления.

</details>

<details>
<summary>154. 2026-10-01T11:17:55.722Z</summary>

Убрал временные выделения памяти при обновлении скалярных индексов. Полный набор тестов прошёл.

В шести парных замерах обновление на 100 000 строках ускорилось в 5 из 6 пар, медианное сокращение времени — **2,3%**. Lin обогнал SQLite в этом сценарии во всех шести запусках. Удаление по-прежнему проигрывает SQLite.

[Отчёт и исходные замеры](../benches/results/2026-10-01-index-visit-ab/report.md).

Общая цель остаётся открытой. Для MSSQL и Kusto нужна внешняя тестовая среда: локальные варианты не поддерживают ARM этой машины. [MSSQL](https://learn.microsoft.com/en-us/sql/linux/quickstart-install-connect-docker?view=sql-server-ver17), [Kusto](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install).

</details>

<details>
<summary>155. 2026-10-01T11:20:07.011Z</summary>

Нашёл вариант, который можно проверить: скалярный индекс и FTS могут прочитать перемещаемую строку по ссылке, а массивы колонок уже переносятся через `swap_remove`. Для карт ID, URI и SPO достаточно сохранить ключи. Попробую обойтись ими вместо копии всей строки.

</details>

<details>
<summary>156. 2026-10-01T11:25:47.878Z</summary>

Сравнение с перестроенными индексами прошло для всех пяти типов коллекций; проверка резервного пути тоже прошла. Финальные бинарники готовы. Запускаю три пары замеров удаления на 1 000, 10 000 и 100 000 строках.

</details>

<details>
<summary>157. 2026-10-01T11:28:47.048Z</summary>

В первых двух парах удаление на 10 000 строках быстрее примерно на 11%; на 100 000 результаты расходятся. На 1 000 строках Lin ускорился сильнее, но там заметно ускорился и контрольный SQLite. Дождусь третьей пары и не буду приписывать всю разницу изменению кода.

</details>

<details>
<summary>158. 2026-10-01T11:29:13.006Z</summary>

Три пары завершены: вариант выиграл на 1 000 и 10 000 строках во всех парах, на 100 000 — в двух из трёх. Сохраню его и повторю тесты в рабочем дереве. Превосходства над SQLite по удалению эти замеры ещё не показывают.

</details>

<details>
<summary>159. 2026-10-01T11:31:04.569Z</summary>

Убрал полное клонирование перемещаемой строки при удалении и повторную регистрацию уже перенесённых колонок. Полный набор тестов прошёл.

На 10 000 строках вариант выиграл все три пары: медианное сокращение времени — **11,9%**. На 100 000 выиграл две из трёх. Фоновая нагрузка менялась, поэтому весь выигрыш нельзя приписать коду.

Удаление пока проигрывает SQLite. [Отчёт и замеры](../benches/results/2026-10-01-borrowed-delete-ab/report.md).

</details>

<details>
<summary>160. 2026-10-01T11:35:07.141Z</summary>

Вариант прошёл полный набор тестов. Отдельный тест подтвердил тот же результат удаления для перемешанных списков длиной до 4 096 элементов, включая пустой список и отсутствующую позицию. Проверю также длительную последовательность удалений и вставок, где хвост списка часто меняется.

</details>

<details>
<summary>161. 2026-10-01T11:39:42.791Z</summary>

Проверка хвоста пока не дала устойчивого выигрыша. При проверке данных заметил ещё одну проблему: Lin вычисляет `ago` заново для каждого блока загрузки, а SQLite получает две заранее заданные даты. Из-за этого индексы строятся на разных распределениях ключей. Проверю способ выровнять данные перед следующими сравнениями.

</details>

<details>
<summary>162. 2026-10-01T11:42:50.118Z</summary>

Длительная последовательность удалений и вставок дала выигрыш во всех трёх парах, а свежие удаления — нет. Сначала выровняю даты в подготовленных наборах Lin и повторю парные замеры: сейчас эти результаты нельзя уверенно объяснить изменением индекса.

</details>

<details>
<summary>163. 2026-10-01T11:47:53.101Z</summary>

Выровнял даты в подготовленных наборах: Lin получает точные значения из `Doc`, после чего индекс перестраивается вне измеряемого участка. Проверки требуют совпадения каждой даты и ровно двух ключей `[wing, ts]`. Сейчас повторю обновление и удаление на этих наборах; бенч вставки это изменение пока не затрагивает.

</details>

<details>
<summary>164. 2026-10-01T11:51:01.636Z</summary>

На выровненном наборе первая пара показала сокращение времени удаления на 100 000 строках с 36,6 до 21,4 мкс. SQLite в том же запуске — 18,6 мкс, поэтому Lin всё ещё медленнее. На малых наборах и обновлениях выигрыш не подтвердился; продолжаю ещё две пары.

</details>

<details>
<summary>165. 2026-10-01T11:53:42.132Z</summary>

Во всех трёх парах на выровненных данных удаление на 100 000 строках быстрее; медианное сокращение времени — 34,9%. Обновления чаще медленнее, хотя контрольный SQLite там тоже замедлялся. Проверю более узкий вариант: проверка хвоста только в `remove_at`, без изменения пути обновления ключа.

</details>

<details>
<summary>166. 2026-10-01T11:57:29.750Z</summary>

Новый вариант меняет только удаление позиции из индекса. В повторных замерах даты и два ключа индекса проверяются до запуска таймера. В отчёте отделю эти результаты от ранних запусков с относительными датами.

</details>

<details>
<summary>167. 2026-10-01T12:00:08.633Z</summary>

В первой паре узкий вариант сократил время удаления на 100 000 строках на 32%, но на меньших наборах проиграл. Обновление тоже оказалось медленнее, хотя его код не менялся; контрольный SQLite заметно колебался. Итог будет зависеть от остальных пар и длительного теста.

</details>

<details>
<summary>168. 2026-10-01T12:05:13.849Z</summary>

Узкий вариант ускорил свежие удаления на 100 000 строках во всех трёх парах, но проиграл две из трёх пар длительной последовательности записей. Пока не буду добавлять эту оптимизацию. Сохраню исправление данных бенча и результаты: они показали, что прежнее сравнение индексов искажалось распределением дат.

</details>

<details>
<summary>169. 2026-10-01T12:08:32.657Z</summary>

Исправил данные бенча: Lin и SQLite теперь получают одинаковые даты и распределение ключей индекса.

На текущем коде удаление из 100 000 строк: **Lin — 24,4 мкс, SQLite — 19,2 мкс**. Это один проверочный запуск, победы пока нет.

Оптимизацию проверки хвоста не применял: она ускорила большие свежие удаления, но дала смешанные результаты на остальных нагрузках.

[Отчёт и замеры](../benches/results/2026-10-01-posting-tail-ab/report.md). Цель по всем восьми системам остаётся открытой.

</details>

<details>
<summary>170. 2026-10-01T12:08:46.184Z</summary>

Следующий пробел — точные даты при измерении вставки: сейчас Lin получает `ago`, а SQL — заданные миллисекунды. Проверю поддержку абсолютного времени, чтобы бенч вставки мог сравнивать одинаковые значения без исправления строк после загрузки.

</details>

<details>
<summary>171. 2026-10-01T12:12:22.957Z</summary>

Добавил литерал `timestamp(миллисекунды)`. Он проходит проверку типов как `time` и хранится в существующем `Cell::Time`. Добавлю проверки границ i64, сравнений через индекс, WAL и повторного открытия; бенч вставки теперь будет проверять точное значение даты.

</details>

<details>
<summary>172. 2026-10-01T12:20:30.297Z</summary>

Полный набор тестов прошёл, включая WAL и повторное открытие с точными датами. Первый запуск бенча зацепил также durable-вставку: она упёрлась в лимит WAL-записи 16 MiB, поэтому этот запуск исключаю из результатов. Повторю сравнение вставки в память точным фильтром; ограничение durable-вставки остаётся отдельным незавершённым пунктом.

</details>

<details>
<summary>173. 2026-10-01T12:24:15.589Z</summary>

Добавил `timestamp(миллисекунды)`. Бенч вставки теперь сравнивает точные даты без исправления строк после загрузки. Тесты, WAL и повторное открытие прошли.

На 10 000 строках, в одном запуске:

- Lin — **10,99 мс**
- SQLite — **10,49 мс**
- DuckDB Appender — **10,44 мс**

Lin пока медленнее. Также обнаружен незакрытый случай durable-вставки: превышение лимита WAL-записи 16 MiB.

[Отчёт и замеры](../benches/results/2026-10-01-absolute-timestamps/report.md).

</details>

<details>
<summary>174. 2026-10-01T12:25:03.686Z</summary>

Разберу durable-вставку, которая превысила лимит WAL-записи. Сначала проверю размер и состав записи, чтобы выбрать способ сократить её без потери данных и нарушения атомарности.

</details>

<details>
<summary>175. 2026-10-01T12:32:03.809Z</summary>

Основную часть большой записи занимают плотные embedding-векторы; у стандартного hashing-embedder большинство элементов — нули. Проверю компактный формат WAL, который сохраняет размер вектора и битовые значения остальных элементов. Кадр останется один, лимит 16 MiB — прежним; отдельно ограничу память при декодировании.

</details>

<details>
<summary>176. 2026-10-01T12:40:24.004Z</summary>

Компактный формат прошёл полный набор тестов. Вставка 10 000 документов теперь помещается в один WAL-кадр и точно воспроизводится на follower. Проверки также покрывают `-0.0`, NaN, пустой вектор, CRC, неверные индексы и чрезмерный заявленный размер. Добавлю отдельное чтение WAL при открытии, затем измерю durable-вставку.

</details>

<details>
<summary>177. 2026-10-01T12:46:15.949Z</summary>

Для тестовых 10 000 документов новый WAL-кадр занимает 4,62 MB; только плотные векторы заняли бы 30,76 MB. Чтение WAL при открытии, follower и checkpoint/reopen прошли, как и полный набор тестов. Запускаю сравнение durable-вставки с SQLite; формат codec 3 требует обновления readers и followers вместе с writer.

</details>

<details>
<summary>178. 2026-10-01T12:51:53.874Z</summary>

Durable-вставка теперь завершается, но в первом замере Lin всё ещё медленнее SQLite. В обоих бенчах добавлена проверка количества и точных полей вне таймера. Повторю запуск ещё в двух отдельных процессах, чтобы получить более устойчивую оценку.

</details>

<details>
<summary>179. 2026-10-01T13:02:25.547Z</summary>

Повторные замеры завершились, тесты прошли. Сейчас свожу цифры в отчёт: вставка 10 000 документов уже проходит, но общего превосходства над SQLite пока нет.

</details>

<details>
<summary>180. 2026-10-01T13:03:26.804Z</summary>

Оптимизировал WAL: разреженные эмбеддинги теперь сохраняются компактно. Вставка 10 000 документов проходит одним атомарным кадром **4,62 МБ**, раньше упиралась в лимит 16 МБ. Тесты прошли.

Durable-бенч, медиана трёх запусков:

| Документов | Lin | SQLite |
|---:|---:|---:|
| 1 000 | 2,97 мс | 1,41 мс |
| 10 000 | 25,94 мс | 13,75 мс |

SQLite пока быстрее. [Полный отчёт](../benches/results/2026-10-01-sparse-vector-wal/report.md).

Перед использованием нового WAL нужно обновить всех читателей и реплики.

</details>

<details>
<summary>181. 2026-10-01T13:03:34.649Z</summary>

Продолжу с расходами на копирование эмбеддингов при записи: проверю путь от строки до WAL и уберу лишние копии, если повторные замеры подтвердят выигрыш.

</details>

<details>
<summary>182. 2026-10-01T13:05:04.892Z</summary>

В кодировании разреженных векторов нашёл отдельный проход для подсчёта ненулевых элементов. Объединил его с записью данных, сохранив формат WAL. Сравниваю исходный и новый бинарники в трёх парах запусков, чередуя порядок.

</details>

<details>
<summary>183. 2026-10-01T13:05:59.583Z</summary>

Проверил объединение проходов при записи эмбеддингов в трёх парных бенчах:

- 1 000 документов: быстрее в 3/3 парах, медианный выигрыш 6,2%.
- 10 000 документов: быстрее лишь в 1/3, медианное ухудшение 1,2%.

Изменение откатил: устойчивого выигрыша нет. Предыдущая оптимизация WAL сохранена. [Замеры и решение](../benches/results/2026-10-01-sparse-single-pass/report.md).

Цель обогнать все восемь систем пока не достигнута.

</details>

<details>
<summary>184. 2026-10-01T13:06:06.122Z</summary>

Следующий шаг — проверить удаление из индекса: поиск позиции внутри списка может тормозить изменения больших таблиц. Сначала посмотрю текущую реализацию и выберу точечную оптимизацию для замера.

</details>

<details>
<summary>185. 2026-10-01T13:07:24.293Z</summary>

Индекс хранил ключ строки, но при удалении линейно искал её позицию среди строк с тем же ключом. Добавил позицию в обратную запись индекса и её обновление при `swap_remove`. Это убирает поиск для любой позиции; теперь проверю корректность и влияние на вставки и изменения.

</details>

<details>
<summary>186. 2026-10-01T13:09:11.649Z</summary>

Проверка удаления обнаружила различие в порядке списков после перестройки индекса. Для новой структуры проверяю и соответствие ключей эталону, и точность каждой сохранённой позиции. Добавил также модельный тест на 3 000 обновлений и удалений; парные бенчи ещё идут.

</details>

<details>
<summary>187. 2026-10-01T13:13:04.691Z</summary>

Три пары завершились. Удаление на 100 000 строк ускорилось в двух парах, но обновление на 10 000 строк замедлилось во всех трёх. Такой обмен пока не подходит для общей цели; сохраню результаты и откатываю эксперимент.

</details>

<details>
<summary>188. 2026-10-01T13:14:19.819Z</summary>

Проверил удаление из индекса за O(1). На 100 000 строк оно ускорилось в 2/3 пар, но обновление на 10 000 строк замедлилось во всех трёх.

Эксперимент откатил; тесты новой структуры прошли, исходная реализация восстановлена. [Полные результаты](../benches/results/2026-10-01-index-same-key/report.md).

Общее превосходство над восемью системами пока не доказано.

</details>

<details>
<summary>189. 2026-10-01T13:14:27.196Z</summary>

Вернусь к записи: проверю, можно ли избежать копирования плотных эмбеддингов при упаковке WAL, сохранив текущий формат и API.

</details>

<details>
<summary>190. 2026-10-01T13:15:55.447Z</summary>

Промежуточный тип WAL оказался внутренним, поэтому заменил копии векторов на ссылки `Arc` на уже сохранённые эмбеддинги. При восстановлении строк они тоже передаются без повторного копирования. Формат данных остаётся прежним; проверяю тесты перед замером.

</details>

<details>
<summary>191. 2026-10-01T13:17:47.740Z</summary>

Тест подтвердил, что MessagePack остаётся побайтно совместимым, а упаковка и восстановление строк используют тот же буфер эмбеддинга. Но первые три пары не показывают устойчивого ускорения записи, и контроль SQLite заметно колеблется. Проведу ещё три пары перед решением.

</details>

<details>
<summary>192. 2026-10-01T13:19:12.364Z</summary>

Проверил удаление копий эмбеддингов при упаковке WAL. Тесты совместимости прошли, но ускорение получилось только в 2 из 6 пар запусков на обоих размерах.

Изменение откатил: устойчивый выигрыш не подтверждён. [Все замеры](../benches/results/2026-10-01-shared-wal-vectors/report.md).

Следующий шаг — профиль времени записи: несколько локальных оптимизаций уже не дали стабильного результата.

</details>

<details>
<summary>193. 2026-10-01T13:19:19.488Z</summary>

Сниму профиль durable-вставки на текущем коде, чтобы найти основной расход времени. Это поможет выбрать следующую оптимизацию после нескольких неудачных локальных экспериментов.

</details>

<details>
<summary>194. 2026-10-01T13:21:20.688Z</summary>

Профиль показал расход в сборе полей WAL: для каждой строки код клонирует имена полей и вставляет их в общий `BTreeSet`, даже если они уже есть. Заменю временные строки на заимствованные имена и буду копировать каждое уникальное имя один раз.

</details>

<details>
<summary>195. 2026-10-01T13:22:49.479Z</summary>

Тесты прошли. Сравниваю сбор полей до и после изменения в шести парных запусках durable-бенча. Профиль включает закрытие БД и checkpoint, поэтому для оценки вставки использую только замеры бенча, где они вынесены за таймер.

</details>

<details>
<summary>196. 2026-10-01T13:24:25.095Z</summary>

Убрал повторное копирование имён полей при сборке WAL.

В шести парных замерах:

- 1 000 документов: быстрее в 5/6 пар, медианный выигрыш 6,1%.
- 10 000 документов: быстрее в 4/6, медианный выигрыш 2,7%; были заметные регрессии.

Изменение оставил для дальнейшей проверки. Тесты прошли, SQLite в durable-вставке пока быстрее. [Профиль и результаты](../benches/results/2026-10-01-durable-profile/report.md).

</details>

<details>
<summary>197. 2026-10-01T13:24:40.317Z</summary>

Продолжу оптимизацию сбора полей WAL: строки с одинаковым набором полей всё ещё повторяют вставки в `BTreeSet`. Проверю быстрый путь для таких строк и сохраню общий путь для строк с разными полями.

</details>

<details>
<summary>198. 2026-10-01T13:25:52.036Z</summary>

Тесты прошли, включая строки одинаковой длины с разными именами полей и поля, впервые появляющиеся позже в пачке. Проверяю быстрый путь в шести парных запусках; формат WAL и fsync не менялись.

</details>

<details>
<summary>199. 2026-10-01T13:27:17.803Z</summary>

Оставил быстрый путь для строк с одинаковыми полями в WAL.

На 10 000 документов Lin ускорился в **6/6 пар**, медианное снижение времени — **5,3%**. Контроль SQLite тоже ускорился на 2%, поэтому весь выигрыш нельзя приписать изменению. На 1 000 строк результат смешанный.

Тесты прошли. Durable-вставка 10 000 строк: Lin **22,48 мс**, SQLite **13,28 мс** — цель ещё не достигнута. [Все замеры](../benches/results/2026-10-01-wal-uniform-fields/report.md).

</details>

<details>
<summary>200. 2026-10-01T13:27:52.115Z</summary>

Проверю следующую часть упаковки WAL: сейчас каждая колонка отдельно ищет своё поле в каждой строке. Для строк с одинаковыми полями попробую один раз собрать ссылки на ячейки и использовать их при кодировании.

</details>

<details>
<summary>201. 2026-10-01T13:31:13.109Z</summary>

Убрал повторные поиски ячеек при упаковке одинаковых строк в WAL. Изменение оставил:

- 10 000 документов: быстрее в **6/6 пар**, медианный выигрыш **5,2%**.
- 1 000 документов: быстрее в **5/6**, медианный выигрыш **3,5%**.

Тесты прошли. Цена — временный массив ссылок на ячейки. SQLite в durable-вставке всё ещё быстрее. [Все замеры](../benches/results/2026-10-01-wal-cell-refs/report.md).

</details>

<details>
<summary>202. 2026-10-01T13:31:38.529Z</summary>

Проверю копирование текстовых значений в WAL. Промежуточные колонки сейчас создают новые `String`, хотя строки уже хранят текст в `Arc<str>`. Попробую разделять эти значения, сохранив прежнюю сериализацию, и сравню запись в парных бенчах.

</details>

<details>
<summary>203. 2026-10-01T13:32:49.583Z</summary>

Изменение компилируется. Добавил проверку побайтной совместимости MessagePack для пустого текста, Unicode и нулевого символа, а также проверку общего буфера при упаковке и восстановлении строки. Полный набор тестов ещё идёт; замеры начну после него.

</details>

<details>
<summary>204. 2026-10-01T13:34:37.694Z</summary>

Убрал копирование текста в промежуточные колонки WAL. Изменение оставил: Lin быстрее в **6/6 пар** на обоих размерах.

Медианное снижение времени: **12,2%** на 1 000 документов и **4%** на 10 000. Контроль SQLite тоже ускорился, поэтому весь процент нельзя приписать изменению.

Тесты и совместимость формата прошли. SQLite пока быстрее в durable-вставке. [Все замеры](../benches/results/2026-10-01-wal-shared-text/report.md).

</details>

<details>
<summary>205. 2026-10-01T13:35:20.288Z</summary>

Перейду к созданию эмбеддингов: проверю, сколько копирований остаётся между генератором и хранилищем. Если удастся убрать копию плотного вектора, проверю побитовое совпадение результатов и общий бенч вставки.

</details>

<details>
<summary>206. 2026-10-01T13:37:39.786Z</summary>

Сборка завершилась, полный прогон тестов ещё работает. Сохраняю побитовую проверку эмбеддингов и начну замеры после завершения тестов, чтобы они не влияли на время вставки.

</details>

<details>
<summary>207. 2026-10-01T13:39:18.000Z</summary>

Тесты подтвердили побитовое совпадение, но замеры не показали выигрыша: на 10 000 документов новый путь проиграл в 5 из 6 пар. Откатываю эксперимент и сохраняю результаты.

</details>

<details>
<summary>208. 2026-10-01T13:39:53.983Z</summary>

Проверил заполнение только ненулевых координат эмбеддинга. Побитовые тесты прошли, но ускорения нет: на 10 000 документов вариант проиграл в **5/6 пар**, медианное ухудшение **1,45%**.

Эксперимент откатил; предыдущие оптимизации WAL сохранены. [Результаты](../benches/results/2026-10-01-embed-sparse-output/report.md).

</details>

<details>
<summary>209. 2026-10-01T13:40:18.077Z</summary>

Проверю, как накопленные изменения выглядят в обычной вставке, а не только durable-режиме: запущу сравнение текущего Lin с SQLite и DuckDB на тех же данных. Это покажет актуальный разрыв и поможет выбрать следующий участок для оптимизации.

</details>

<details>
<summary>210. 2026-10-01T13:41:29.099Z</summary>

Обычная вставка на 10 000 строк близка к конкурентам: Lin 10,65 мс, SQLite 10,45 мс, DuckDB Appender 10,32 мс. На 1 000 строк Lin быстрее DuckDB, но уступает SQLite.

Диагностические варианты указывают на заметную стоимость эмбеддингов и индексов; отключение компонентов не считаю победой. MSSQL и Kusto по-прежнему не настроены.

</details>

<details>
<summary>211. 2026-10-01T13:42:49.822Z</summary>

Проверяю вычисление номера координаты: для стандартной размерности 768 явно выделил деление на константу, чтобы компилятор мог упростить его. Для остальных размерностей путь прежний. Побитовые тесты и парные замеры покажут, стоит ли оставлять изменение.

</details>

<details>
<summary>212. 2026-10-01T13:44:34.898Z</summary>

Обновил бенч обычной вставки на 10 000 документов:

| Lin | SQLite | DuckDB Appender |
|---:|---:|---:|
| 10,65 мс | 10,45 мс | 10,32 мс |

Разрыв небольшой, но Lin пока проигрывает. [Полные результаты](../benches/results/2026-10-01-native-after-wal/report.md).

Проверил упрощение вычисления координаты эмбеддинга — устойчивого выигрыша нет, откатил. MSSQL и Kusto ещё не настроены; общая цель остаётся недоказанной.

</details>

<details>
<summary>213. 2026-10-01T13:44:44.559Z</summary>

Проверю расходы на FTS при массовой вставке. В диагностических замерах они заметны; посмотрю, какие повторные обходы и временные объекты можно убрать, сохранив поисковый индекс.

</details>

<details>
<summary>214. 2026-10-01T13:46:50.183Z</summary>

У добавления нового термина в FTS два поиска по хеш-таблице: сначала `get_mut`, затем `insert`. Проверю заимствованный `entry_ref`, который позволяет выделять строку только для нового термина. Замеры должны учесть и вставку, и влияние смены таблицы на чтение и изменения.

</details>

<details>
<summary>215. 2026-10-01T13:48:28.335Z</summary>

Новая таблица компилируется. Алгоритм списков позиций и формат FTS не менялись; полный набор тестов ещё идёт. После него сравню вставку, лексический поиск и изменения строк, поскольку смена таблицы затрагивает все три пути.

</details>

<details>
<summary>216. 2026-10-01T13:50:22.025Z</summary>

Вставка не показала устойчивого выигрыша: на 10 000 строк новый вариант быстрее только в 2 из 6 пар, медианное время почти не изменилось. Завершу проверку чтения и изменений строк, после чего сохраню результаты и верну исходную таблицу.

</details>

<details>
<summary>217. 2026-10-01T13:51:45.339Z</summary>

Проверил новый путь хеш-таблицы FTS на вставке, чтении и изменениях строк. Выигрыш не подтвердился: вставка почти не изменилась, все три случая FTS-чтения проиграли во всех парах.

Эксперимент и зависимость откатил. Предыдущие оптимизации WAL сохранены. [Замеры и результаты тестов](../benches/results/2026-10-01-fts-entry-ref/report.md).

</details>

<details>
<summary>218. 2026-10-01T13:52:12.383Z</summary>

Проверю регистрацию служебных колонок при массовой вставке: данные уже собраны в строки, но хранилище снова обходит их для ID, URI и других полей. Посмотрю, можно ли сократить этот путь без изменения поведения.

</details>

<details>
<summary>219. 2026-10-01T13:54:00.245Z</summary>

Для массовой вставки документов заменил пять отдельных поисков полей в `BTreeMap` на один обход строки. Правила для отсутствующих и нетекстовых значений сохранил. После тестов сравню полную вставку, чтобы оценить выигрыш от этого обхода.

</details>

<details>
<summary>220. 2026-10-01T13:55:35.554Z</summary>

Тесты прошли. Идут шесть парных запусков полной вставки на 1 000 и 10 000 документов с проверкой данных вне таймера. Сравнение включает SQLite и DuckDB Appender; порядок исходного и нового бинарников чередуется.

</details>

<details>
<summary>221. 2026-10-01T13:57:08.366Z</summary>

Один обход ускорил вставку в 6 из 6 пар на обоих размерах, примерно на 6% по медиане. Это ещё не устойчивая победа над SQLite и DuckDB. Добавлю проверку служебных колонок на пропущенных, нетекстовых и повторяющихся значениях и проверю durable-вставку.

</details>

<details>
<summary>222. 2026-10-01T13:59:34.476Z</summary>

Проверка служебных колонок прошла: результат совпадает с регистрацией по одной строке. Первые durable-замеры смешанные и показывают замедление на 1 000 строк; контроль SQLite тоже сильно колеблется. Добавлю ещё три пары, чтобы лучше оценить этот риск.

</details>

<details>
<summary>223. 2026-10-01T14:00:40.198Z</summary>

Оставил регистрацию служебных колонок документов за один обход строки.

Обычная вставка ускорилась в **6/6 пар** на обоих размерах, медианный выигрыш — около **6%**. Durable на 10 000 строк: быстрее в **5/6**, примерно **4,2%**. На 1 000 durable-строк результат хуже; это остаётся нерешённым.

Тесты прошли. Устойчивого превосходства над всеми конкурентами ещё нет. [Подробные замеры](../benches/results/2026-10-01-doc-slab-scan/report.md).

</details>

<details>
<summary>224. 2026-10-01T14:00:56.237Z</summary>

Проверю проверку уникальности ID и URI в массовой вставке. Если целевая коллекция пуста, поиск конфликтов с существующими строками может быть лишним; дубликаты внутри самой пачки должны проверяться всегда.

</details>

<details>
<summary>225. 2026-10-01T14:02:56.866Z</summary>

Изменение пропускает только поиск по существующим строкам, когда коллекция пуста. Дубликаты внутри пачки проверяются по-прежнему. Добавил тесты конфликтов ID и URI для пустой и заполненной коллекции, с проверкой отката и успешной вставки после ошибки.

</details>

<details>
<summary>226. 2026-10-01T14:04:13.107Z</summary>

Бенч не подтвердил ускорение: на 10 000 документов вариант проиграл в 5 из 6 пар, медианное ухудшение — 3,4%. Откатываю оптимизацию. Тесты уникальности и отката оставлю как проверку для следующих изменений.

</details>

<details>
<summary>227. 2026-10-01T14:04:56.435Z</summary>

Проверил пропуск поиска конфликтов в пустой коллекции. Ускорения нет: на 10 000 документов вариант проиграл в **5/6 пар**, медианное ухудшение — **3,4%**. Оптимизацию откатил.

Оставил прошедшие тесты дубликатов ID/URI и атомарного отката. [Все замеры](../benches/results/2026-10-01-empty-store-uniqueness/report.md).

</details>

<details>
<summary>228. 2026-10-01T14:06:24.361Z</summary>

Посмотрю следующий крупный расход в durable-записи: размер WAL после разреженного кодирования. Проверю текущий формат и зависимости, чтобы выбрать способ уменьшить кадр без потери точности и ослабления fsync.

</details>

<details>
<summary>229. 2026-10-01T14:08:52.009Z</summary>

Проверяю сжатие крупных разреженных кадров WAL через Zstd уровня 1. Исходные ограничения размера и fsync сохранены; новый декодер ограничивает размер распаковки. Сначала проверю replay и репликацию, затем сравню стоимость сжатия с экономией на записи.

</details>

<details>
<summary>230. 2026-10-01T14:13:14.517Z</summary>

Сжатие уменьшило кадр почти в 11 раз, но замедлило durable-вставку во всех шести парах: примерно на 17% для 1 000 документов и на 13% для 10 000. Для цели по скорости это не подходит; откатываю кодек и зависимость.

</details>

<details>
<summary>231. 2026-10-01T14:13:53.725Z</summary>

Проверил сжатие WAL через Zstd. Кадр на 10 000 документов уменьшился **с 4,62 МБ до 427 КБ**, тесты восстановления прошли.

Но вставка замедлилась во всех **6/6 пар**: медианно на **16,8%** для 1 000 документов и **13,4%** для 10 000. Кодек и зависимость откатил. [Все замеры](../benches/results/2026-10-01-wal-zstd/report.md).

</details>

<details>
<summary>232. 2026-10-01T14:14:19.677Z</summary>

Сниму профиль обычной вставки после последних изменений. Разрыв с SQLite и DuckDB небольшой, но ещё не закрыт; профиль поможет выбрать следующий участок по затратам.

</details>

<details>
<summary>233. 2026-10-01T14:17:34.992Z</summary>

Профиль выделил построение `Row` из полей записи как крупный расход. Проверю сортировку полей на этапе `prepare` и сборку карты целиком при исполнении. Проверку исходного AST и вычисление `now` сохраню; дублирующиеся поля должны по-прежнему брать последнее значение.

</details>

<details>
<summary>234. 2026-10-01T14:20:14.786Z</summary>

Тесты прошли, включая последнее из трёх повторяющихся полей и вычисление `now` при исполнении. Начинаю парные замеры полной вставки. Сортировка выполняется при подготовке запроса и не меняет набор данных или проверок.

</details>

<details>
<summary>235. 2026-10-01T14:21:54.250Z</summary>

Новый вариант быстрее в 4 из 6 пар, но медианный выигрыш около 0,6%; контрольные движки колеблются сильнее. Убедительного улучшения нет. Откатываю сортировку и новый способ сборки карты, сохраняя профиль и проверку семантики подготовленной вставки.

</details>

<details>
<summary>236. 2026-10-01T14:23:03.785Z</summary>

Снял профиль и проверил перенос сортировки полей в `prepare`. Убедительного ускорения нет: медианный выигрыш около **0,6%**, меньше колебаний контрольных движков. Оптимизацию откатил.

Оставил прошедший тест последнего дублирующегося поля и вычисления `now` при исполнении. [Профиль](../benches/results/2026-10-01-native-profile-current/report.md) · [Бенчи](../benches/results/2026-10-01-prepared-sorted-records/report.md).

</details>

<details>
<summary>237. 2026-10-01T14:26:30.258Z</summary>

Нашёл ограничение в бенчмарке PostgreSQL: он удаляет таблицы с фиксированными именами. Изолирую каждый запуск в собственную схему с очисткой после завершения. Так можно будет проверять PostgreSQL без риска затронуть чужие таблицы, сохранив обычную запись в WAL.

</details>

<details>
<summary>238. 2026-10-01T14:30:17.107Z</summary>

Изоляция PostgreSQL компилируется. Проверю её на живом сервере в отдельной тестовой БД: оставлю контрольные таблицы в `public`, запущу бенч и проверю, что таблицы сохранились, а созданные схемы удалились. MySQL в этом запуске явно отключён — его старый стенд требует отдельной изоляции.

</details>

<details>
<summary>239. 2026-10-01T14:34:06.887Z</summary>

Живая проверка прошла: все 11 PostgreSQL-сценариев завершились, пять контрольных таблиц в `public` сохранились, созданных схем осталось ноль. Это подтверждает изоляцию стенда; один замер на сценарий не доказывает превосходство по скорости.

</details>

<details>
<summary>240. 2026-10-01T14:34:53.036Z</summary>

Исправил изоляцию PostgreSQL-бенча: каждый fixture использует собственную схему вместо удаления таблиц в `public`.

Проверил на живом сервере: **11 сценариев прошли**, контрольные таблицы сохранились, временных схем осталось **0**. [Отчёт](../benches/results/2026-10-01-pg-owned-schema/report.md).

Это подготовка к достоверным замерам, не победа по скорости. Для MSSQL и Kusto ещё нужны тестовые подключения.

</details>

<details>
<summary>241. 2026-10-01T14:35:03.214Z</summary>

Усилю проверку PostgreSQL-вставки: добавлю чтение и сверку всех записанных полей вне таймера. После этого запущу повторные сравнения с Lin.

</details>

<details>
<summary>242. 2026-10-01T14:37:24.814Z</summary>

В текущем стенде PostgreSQL вставляет строки по одной внутри транзакции. Добавлю также binary `COPY` с теми же индексами и проверкой полей: это более подходящий путь для массовой загрузки. Одного сравнения с построчным INSERT недостаточно для общей победы.

</details>

<details>
<summary>243. 2026-10-01T14:40:17.793Z</summary>

COPY и проверка полей компилируются. Проверка читает данные отдельным подключением после commit, вне таймера. Запускаю три независимых процесса на 1 000 и 10 000 строк; сохраню также версию и настройки PostgreSQL для интерпретации результатов.

</details>

<details>
<summary>244. 2026-10-01T14:44:13.591Z</summary>

Замеры завершились, проверка всех полей прошла. В этих трёх процессах Lin быстрее PostgreSQL COPY на обоих размерах и SQLite на 10 000 строк. На 1 000 строк SQLite быстрее во всех трёх; DuckDB на 10 000 быстрее Lin в одном процессе. Сведу результаты без общей победной формулировки.

</details>

<details>
<summary>245. 2026-10-01T14:48:27.285Z</summary>

Свежий бенч: медианы трёх независимых запусков, время вставки в мс — меньше лучше.

| Строк | Lin | SQLite | DuckDB Appender | PostgreSQL COPY |
|---|---:|---:|---:|---:|
| 1 000 | 0,963 | **0,828** | 1,302 | 4,716 |
| 10 000 | **10,257** | 10,680 | 10,502 | 28,885 |

На 10 000 строк Lin обогнал SQLite в 3/3 запусках, DuckDB — в 2/3. На 1 000 SQLite пока быстрее.

Добавил PostgreSQL COPY и точную проверку всех записанных полей вне таймера. PostgreSQL работал в Docker на tmpfs — это не замер физического диска.

[Полный отчёт и исходные результаты](../benches/results/2026-10-01-pg-validated-insert/report.md). Победа над всеми восемью системами пока не подтверждена.

</details>

<details>
<summary>246. 2026-10-01T14:51:53.548Z</summary>

Продолжу с вставки 1 000 строк, где SQLite быстрее. Проверю текущий код и профиль затрат, затем сравню изменение с исходным вариантом.

</details>

<details>
<summary>247. 2026-10-01T14:53:49.961Z</summary>

В пакетной вставке FTS создаёт новый буфер приведения к нижнему регистру для каждой строки. Попробую переиспользовать один буфер на пакет. Сравню скорость и проверю, что токены и результаты поиска совпадают.

</details>

<details>
<summary>248. 2026-10-01T14:56:09.327Z</summary>

Обычный бенч содержит только строчные заголовки, поэтому почти не задействует этот буфер. Добавляю отдельный режим со смешанным регистром и проверкой точных заголовков. Замерю оба режима: выигрыш на новом сценарии и возможную регрессию на прежнем.

</details>

<details>
<summary>249. 2026-10-01T14:58:03.555Z</summary>

На смешанном регистре новый вариант быстрее в 6/6 пар запусков: медианное снижение времени — 2,96% для 1 000 строк и 3,90% для 10 000. На строчных заголовках устойчивого выигрыша нет; колебания близки к контрольному SQLite. Оставляю изменение ради подтверждённого выигрыша при нормализации текста и завершаю проверку тестами.

</details>

<details>
<summary>250. 2026-10-01T14:59:45.350Z</summary>

Оптимизировал FTS при пакетной вставке: буфер приведения текста к нижнему регистру теперь переиспользуется между строками.

На заголовках со смешанным регистром:

- 1 000 строк: время снизилось на **2,96%**.
- 10 000 строк: на **3,90%**.
- Улучшение в **6/6 пар запусков** для обоих размеров.

На строчных заголовках устойчивого выигрыша нет. SQLite на 1 000 строк пока быстрее. Все тесты workspace прошли.

[Отчёт и сырые замеры](../benches/results/2026-10-01-fts-slab-scratch/report.md).

</details>

<details>
<summary>251. 2026-10-01T15:46:38.928Z</summary>

Изменения и результаты бенчей сохранены в коммите `8126c0c`. При вливании `themoretheless-check-problems` возникли конфликты в пяти файлах: `src/embed.rs`, `src/exec.rs`, `src/fts.rs`, `src/persist.rs`, `src/store.rs`. Пуш и удаление не выполнял.

Остановился согласно разделу «Вливание в основную ветку» [SKILL.md](../../../../.skills/skills/push-or-clear/SKILL.md): «При конфликте — остановиться, не разрешать конфликт молча».

</details>

<details>
<summary>252. 2026-10-01T15:46:52.753Z</summary>

Всё сохранено в коммите `8126c0c`: оптимизации, тесты и результаты бенчей. Stash отсутствуют.

Слияние `themoretheless-check-problems` остановилось на конфликтах в `embed.rs`, `exec.rs`, `fts.rs`, `persist.rs`, `store.rs`. Пуш и удаление не выполнялись.

Разобрать конфликты с сохранением текущих оптимизаций или отменить слияние и запушить только текущий `main`?

Подтверждение требуется по [SKILL.md](../../../../.skills/skills/push-or-clear/SKILL.md): «При конфликте — остановиться, не разрешать конфликт молча».

</details>

<details>
<summary>253. 2026-10-01T17:16:29.206Z</summary>

Первый процесс завершил сравнение Lin с SQLite на 100 000 строках: результаты всех шести запросов совпали, Lin быстрее в каждом. Ещё идут DuckDB, pandas и следующие независимые процессы. MSSQL и Kusto по-прежнему без настроенных подключений, поэтому общий результат по восьми системам остаётся неподтверждённым.

</details>

<details>
<summary>254. 2026-10-01T17:17:58.274Z</summary>

Проверил коммит `8126c0c` отдельно от конфликтующего слияния: на 100 000 строк Lin быстрее SQLite, DuckDB и pandas во всех шести сценариях чтения — **18/18 сравнений**, три независимых процесса, точная проверка результатов.

[Свежий отчёт](../benches/results/2026-10-01-committed-local-read-audit/report.md).

Общая цель ещё не достигнута: SQLite быстрее на вставке 1 000 строк; MSSQL и Kusto не измерены. Слияние и пуш остаются в ожидании вашего выбора по конфликтам.

</details>

<details>
<summary>255. 2026-10-01T17:18:16.261Z</summary>

Продолжу искать выигрыш на вставке 1 000 строк в отдельной копии сохранённого коммита. Конфликтующие файлы текущего слияния трогать не буду; изменение оставлю только при подтверждённом улучшении.

</details>

<details>
<summary>256. 2026-10-01T17:20:07.124Z</summary>

Следующая гипотеза — убрать создание общей строки `title + body + snippet` перед вычислением embedding. Для встроенного хеширующего embedder попробую обработать части напрямую, сохранив порядок признаков и переходы через пробелы. Пользовательские embedder продолжат получать прежний текст через совместимый путь. Проверю побитовое совпадение векторов.

</details>

<details>
<summary>257. 2026-10-01T17:23:15.432Z</summary>

Кандидат готов в отдельной копии. Проверка сравнивает биты векторов для пустых полей, пробелов, Unicode, переводов строк и длинного текста. Если она пройдёт, замерю обычные и durable-вставки; пока выигрыша скорости не заявляю.

</details>

<details>
<summary>258. 2026-10-01T17:26:29.339Z</summary>

Побитовое сравнение векторов прошло, но замеры не дали устойчивого выигрыша: вставка 1 000 строк чаще замедлялась. Отклоняю этот вариант; сохраню патч и результаты эксперимента, чтобы не повторять его.

</details>

<details>
<summary>259. 2026-10-01T17:27:21.765Z</summary>

Проверил обработку embedding без сборки общей строки. Векторы побитово совпали, но вариант отклонил: durable-вставка 1 000 строк замедлилась в **5/6 запусков**, медианно на **2,30%**. На 10 000 был выигрыш 3,25%, но он не решает отставание на малых пакетах.

[Замеры и экспериментальный патч](../benches/results/2026-10-01-segmented-embedding/report.md). Текущий код и ожидающее разрешения слияние не изменял; общая цель остаётся открытой.

</details>

<details>
<summary>260. 2026-10-01T17:34:04.199Z</summary>

Вариант с hybrid top-k собрался, библиотечные и CLI-тесты прошли; интеграционные тесты ещё идут. Результаты скорости пока не измеряю, чтобы не смешивать бенч с проверками. Вариант и новые тесты сохранены отдельно от конфликтующего слияния.

</details>

<details>
<summary>261. 2026-10-01T21:39:36.570Z</summary>

Проверю результат запущенных интеграционных тестов. Если они завершились успешно, замерю hybrid top-k отдельно от тестов.

</details>

<details>
<summary>262. 2026-10-01T21:39:55.734Z</summary>

Тест остановился на моей ошибке в синтаксисе: hybrid-поиск вызывается как `search "wal"`, а не `search hybrid "wal"`. Исправлю тест и повторю проверку; о корректности или скорости варианта пока выводов нет.

</details>

<details>
<summary>263. 2026-10-01T21:41:22.119Z</summary>

Исправленный вариант повторно проверяется. В нём также сохранён новый метод `embed_batch_owned` из ветки; текущий путь вставки продолжает использовать прежний `embed_batch`. Бенч начну после завершения тестов.

</details>

<details>
<summary>264. 2026-10-01T21:42:25.839Z</summary>

В подготовленном варианте hybrid-поиск ускорился: **13,95 → 8,29 мс**, медианное снижение времени **40,44%**, выигрыш в **6/6 пар запусков**. Все тесты workspace прошли.

[Отчёт и проверенный патч](../benches/results/2026-10-01-merge-proposal/report.md).

Вариант пока в отдельной копии. Для применения к текущему слиянию ожидается ваш выбор по конфликтам — остановка требуется [push-or-clear](../../../../.skills/skills/push-or-clear/SKILL.md). Победа над всеми восемью движками ещё не доказана.

</details>

<details>
<summary>265. 2026-10-01T21:42:44.189Z</summary>

Проверю, изменились ли условия для продолжения: состояние слияния и наличие подключений MSSQL/Kusto. Проверенный вариант разрешения конфликтов уже готов; без вашего выбора скилл не разрешает применять его.

</details>

<details>
<summary>266. 2026-10-01T21:42:55.546Z</summary>

Условия не изменились: слияние конфликтует, подключений MSSQL/Kusto нет.

Для продолжения нужен ваш выбор: **применить [проверенный вариант разрешения конфликтов](../benches/results/2026-10-01-merge-proposal/report.md)** или отменить слияние. Для сравнения с MSSQL/Kusto нужны тестовые подключения через переменные окружения.

Подтверждение требуется по [push-or-clear](../../../../.skills/skills/push-or-clear/SKILL.md): «При конфликте — остановиться, не разрешать конфликт молча». Цель пока не достигнута.

</details>

<details>
<summary>267. 2026-10-01T21:43:14.010Z</summary>

Блокировка сохраняется: пять конфликтующих файлов, MSSQL/Kusto не настроены. Проверенный вариант готов, но для его применения нужен ваш выбор по слиянию.

Остановка при конфликте прямо требуется [push-or-clear](../../../../.skills/skills/push-or-clear/SKILL.md). Цель остаётся незавершённой.

</details>

<details>
<summary>268. 2026-10-01T21:43:33.251Z</summary>

Цель отмечена как **заблокированная**: три проверки подряд подтвердили те же препятствия.

Для продолжения нужны:

- Решение применить [проверенный вариант слияния](../benches/results/2026-10-01-merge-proposal/report.md) либо отменить слияние.
- Тестовые подключения MSSQL/Kusto через переменные окружения.

Остановка при конфликте требуется [push-or-clear](../../../../.skills/skills/push-or-clear/SKILL.md). Все изменения сохранены; победа над восемью движками ещё не подтверждена.

</details>

<details>
<summary>269. 2026-10-01T21:45:41.258Z</summary>

Новые отчёты и проверенный вариант слияния сохранены в коммите `e5ec3eb` на ветке `codex/benchmark-evidence-2026-10-02`. Текущее слияние всё ещё конфликтует; пуш и удаление не выполнены.

**Применить подготовленный вариант разрешения конфликтов и продолжить пуш?**

Подтверждение требуется по [SKILL.md](../../../../.skills/skills/push-or-clear/SKILL.md): «При конфликте — остановиться, не разрешать конфликт молча».

</details>

<details>
<summary>270. 2026-10-01T21:57:53.830Z</summary>

Конфликты разрешены. Сохранил текущие оптимизации, добавил hybrid top-k и тест восстановления большого пакета из ветки. Бенч собирается; тесты итогового кода ещё идут.

</details>

<details>
<summary>271. 2026-10-01T21:59:05.737Z</summary>

Проверки удаления и embeddings прошли, в том числе совместимость векторов и вызов пользовательского batch API. В остальных worktree изменений кода нет. После оставшихся тестов создам merge-коммит и выполню пуш.

</details>

<details>
<summary>272. 2026-10-01T22:01:21.430Z</summary>

Влил и запушил в `main`: [f6abf05](https://github.com/themoretheless/lin/commit/f6abf053b5f643bb440b44ef66c75466a87fb2d2). Все тесты прошли, бенч собирается, рабочее дерево чистое. Отчёты сохранены.

Удалить влитые ветки `codex/benchmark-evidence-2026-10-02` и `themoretheless-cuddly-happiness`? Подтверждение требуется разделом «Зачистка» [SKILL.md](../../../../.skills/skills/push-or-clear/SKILL.md).

</details>

<details>
<summary>273. 2026-10-01T22:42:15.351Z</summary>

Новых изменений кода нет. `main` синхронизирован с `origin/main` на `f6abf05`; всё сохранено и запушено.

Удалить влитые локальные ветки `codex/benchmark-evidence-2026-10-02` и `themoretheless-cuddly-happiness`?

Для удаления [SKILL.md](../../../../.skills/skills/push-or-clear/SKILL.md) требует «дождаться явного подтверждения пользователя». Worktree пока оставлены.

</details>

<details>
<summary>274. 2026-10-02T00:26:24.291Z</summary>

Слияние завершено, можно продолжать работу в `main`. Следующий шаг — безопасный native-бенч MySQL: проверю изоляцию таблиц, затем добавлю сравнение пакетной вставки с точной проверкой записанных значений.

</details>

<details>
<summary>275. 2026-10-02T00:31:03.766Z</summary>

MySQL-фикстуры теперь используют уникальные обычные InnoDB-таблицы. Добавил native-вставку одним подготовленным запросом: транзакция и commit входят в замер, подготовка — снаружи. Проверю сохранность контрольных таблиц и все шесть записанных полей через отдельное соединение.

</details>

<details>
<summary>276. 2026-10-02T00:33:04.423Z</summary>

Замеры завершились. Lin быстрее MySQL multi-row INSERT в 3/3 процессах на обоих размерах: примерно 0,94 против 6,12 мс для 1 000 строк и 10,35 против 70,39 мс для 10 000. Контрольные таблицы не изменились, временные таблицы удалены. SQLite на 1 000 строк и DuckDB Appender на 10 000 пока быстрее Lin.

</details>

<details>
<summary>277. 2026-10-02T00:35:37.653Z</summary>

Добавил безопасные MySQL-фикстуры и native multi-row INSERT с точной проверкой всех полей.

| Строк | Lin | MySQL batch |
|---|---:|---:|
| 1 000 | **0,913 мс** | 5,987 мс |
| 10 000 | **9,639 мс** | 68,460 мс |

Lin быстрее в **3/3 процессах** на обоих размерах. Стенд MySQL — Docker с tmpfs; это сравнение API, не физического диска.

[Отчёт и сырые результаты](../benches/results/2026-10-02-mysql-owned-native/report.md). Изменения пока локальные. SQLite на 1 000 строк всё ещё быстрее; общая цель по восьми системам не достигнута.

</details>

<details>
<summary>278. 2026-10-02T20:56:41.776Z</summary>

Вернусь к вставке 1 000 строк, где Lin отстаёт от SQLite. Проверю лишние копирования при проверке уникальности и сравню кандидат с текущим вариантом в нескольких независимых запусках.

</details>

<details>
<summary>279. 2026-10-02T20:59:53.997Z</summary>

Первый базовый прогон завершён. Идёт парный кандидат: по 24 свежие вставки на размер. Запущены шесть чередующихся пар для обычной вставки и шесть для durable-вставки; пока выводов по скорости нет.

</details>

<details>
<summary>280. 2026-10-02T21:02:46.489Z</summary>

Кандидат дал небольшой выигрыш на обычной вставке: около 1,5% для 1 000 строк. Но durable-вставка на том же размере замедлилась в 5/6 пар, медианно на 3,26%. Отклоняю изменение и возвращаю прежний код; причина замедления этими замерами не установлена.

</details>

<details>
<summary>281. 2026-10-02T21:03:12.706Z</summary>

Проверил удаление лишних копирований `Arc` при вставке. Обычная вставка 1 000 строк улучшилась на **1,54%**, но durable-вставка замедлилась на **3,26%** в 5/6 пар запусков.

Изменение отклонил, прежний код восстановил. [Отчёт и патч эксперимента](../benches/results/2026-10-03-insert-borrowed-text/report.md). Отставание от SQLite на малых вставках пока не устранено.

</details>

<details>
<summary>282. 2026-10-02T21:04:36.001Z</summary>

Добавлю native-вставку MongoDB, чтобы сравнение охватывало её пакетный API, а не только чтения. Проверю одинаковые данные и ограничения уникальности; подготовку и проверку результата вынесу из таймера.

</details>

<details>
<summary>283. 2026-10-02T21:14:23.359Z</summary>

Применю push-or-clear: проверю новые изменения и результаты бенчей, затем закоммичу и запушу готовую работу.

</details>

<details>
<summary>284. 2026-10-02T21:14:39.472Z</summary>

Продолжу оптимизацию и включу проверенный результат в пуш. Свежий native-бенч показал Lin быстрее MongoDB в 5,71× на 1 000 документов и в 4,84× на 10 000. Следующую правку проверю парными замерами до и после.

</details>

<details>
<summary>285. 2026-10-02T21:16:17.560Z</summary>

Проверяю сокращение выделений памяти при удалении документов: короткий список FTS-токенов можно хранить на стеке. Длинные тексты сохранят прежний путь через heap. Оставлю правку только при подтверждённом ускорении.

</details>

<details>
<summary>286. 2026-10-02T21:18:38.739Z</summary>

Первый вариант не дал устойчивого выигрыша на 1 000 строках — откатываю его. Нашёл ещё один лишний расход в FTS-поиске: уже отсортированные postings проходят через BTreeSet. Проверю прямое слияние списков с сохранением сортировки и уникальности.

</details>

<details>
<summary>287. 2026-10-02T21:21:02.832Z</summary>

Прямое слияние дало устойчивый результат: поиск частого термина ускорился на 12,1%, выиграв во всех 6 парных запусках. Выборочный поиск тоже стал быстрее в 5 из 6 пар. Проверяю весь workspace и корректность запросов с несколькими терминами после изменений postings.

</details>

<details>
<summary>288. 2026-10-02T21:22:29.887Z</summary>

Все тесты workspace прошли, бенч собирается, проверки Python тоже зелёные. Сохраняю FTS-ускорение, native-бенчи MySQL/MongoDB и результаты отклонённых экспериментов, затем пушу в main.

</details>

<details>
<summary>289. 2026-10-02T21:23:38.439Z</summary>

Оптимизировал FTS: заменил BTreeSet прямым слиянием отсортированных postings.

Поиск частого термина ускорился на **12,1%**, выигрыш в **6/6** парных запусков. [Результаты бенча](../benches/results/2026-10-03-fts-sorted-union/report.md).

Все тесты workspace прошли. Запушено в `main`: `9946179`. Рабочее дерево чистое; ветки и worktree сохранены.

</details>

<details>
<summary>290. 2026-10-02T22:25:14.407Z</summary>

Продолжу сравнение с конкурентами: проверю, где Lin всё ещё проигрывает, и выберу следующий участок для оптимизации по замерам.

</details>

<details>
<summary>291. 2026-10-02T22:26:02.020Z</summary>

В последних native-замерах вставка 1 000 документов всё ещё уступала SQLite. Проверяю затраты default embedding: отдельный массив `seen` можно убрать, потому что веса признаков положительные, а после каждого текста затронутые ячейки обнуляются. Проверю побитовое совпадение векторов и скорость вставки.

</details>

<details>
<summary>292. 2026-10-02T22:27:17.693Z</summary>

Побитовое сравнение векторов прошло. В native-вставке предварительный выигрыш составил 3,8% на 1 000 документов и 4,7% на 10 000. Сейчас идут парные замеры с WAL, чтобы проверить эффект на durable-вставке.

</details>

<details>
<summary>293. 2026-10-02T22:28:05.804Z</summary>

WAL-замеры показали регрессию на 1 000 документов: замедление на 5,3%, проигрыш в 5 из 6 пар. Эту правку отклоняю. Следующий приоритет — small durable-вставка: там разрыв со SQLite заметно больше, чем в native-вставке.

</details>

<details>
<summary>294. 2026-10-02T22:28:53.999Z</summary>

Проверил оптимизацию embedding: native-вставка ускорилась на 3,8–4,7%, но durable-вставка 1k замедлилась на 5,3%. Правку откатил.

[Замеры сохранены](../benches/results/2026-10-03-embed-zero-marker/report.md) и запушены: `cc5008b`.

Цель ещё не достигнута: small inserts проигрывают SQLite; MSSQL/Kusto не проверены без тестовых endpoints. Следующий приоритет — затраты WAL на малых вставках.

</details>

<details>
<summary>295. 2026-10-02T22:29:55.269Z</summary>

Разберу стоимость WAL для вставки 1 000 документов: отделю кодирование от записи и синхронизации файла, чтобы выбрать оптимизацию по измерениям.

</details>

<details>
<summary>296. 2026-10-02T22:32:09.057Z</summary>

Профиль WAL для 1 000 документов: кодирование — 0,532 мс, checksum — 0,012 мс, запись — 0,046 мс, синхронизация — 0,883 мс. На 10 000 кодирование занимает уже 5,662 мс. Сокращение числа write-вызовов даст мало; следующий участок — кодирование sparse embedding, без ослабления durability.

</details>

<details>
<summary>297. 2026-10-02T22:33:16.011Z</summary>

Замерил фазы WAL в трёх независимых процессах:

| Документы | Кодирование | Запись | Синхронизация |
|---|---:|---:|---:|
| 1 000 | 0,532 мс | 0,046 мс | 0,883 мс |
| 10 000 | 5,662 мс | 0,274 мс | 1,341 мс |

Следующий приоритет — кодирование sparse embedding. [Профиль и ограничения измерений](../benches/results/2026-10-03-wal-profile/report.md) запушены: `6b3e622`. Временная инструментализация удалена.

Победа над всеми восемью движками пока не доказана.

</details>

<details>
<summary>298. 2026-10-03T09:13:01.510Z</summary>

Проверю ускорение записи sparse embedding: пропуск блоков, где все значения — положительный ноль. Формат WAL сохраню; отдельно проверю `-0.0`, NaN и точность восстановления, затем сравню durable-вставку до и после.

</details>

<details>
<summary>299. 2026-10-03T09:15:10.895Z</summary>

Проверки WAL прошли: `-0.0`, NaN и значения на границах блоков восстанавливаются побитово; checksum и обработка обрезанных записей тоже проходят. Идут 6 парных запусков на 1k и 10k документов с неизменным режимом Full sync.

</details>

<details>
<summary>300. 2026-10-03T09:17:17.443Z</summary>

На 10 000 документов durable-вставка ускорилась на 4,6%, выиграв во всех 6 парах; SQLite-контроль в этих запусках немного замедлился. На 1 000 эффект слабее и неустойчив — около 0,9%. Оставлю правку для большого batch и прогоню весь workspace.

</details>

<details>
<summary>301. 2026-10-03T09:19:32.792Z</summary>

Проверки проходят; подтверждённый выигрыш пока относится к большому durable batch. На малой вставке устойчивого ускорения нет, и SQLite остаётся быстрее. После завершения тестов сохраню правку вместе с замерами.

</details>

<details>
<summary>302. 2026-10-03T09:21:32.291Z</summary>

Ускорил durable-вставку 10 000 документов на **4,6%**, выигрыш в **6/6** парных запусков. WAL пропускает нулевые блоки embedding; формат и Full sync сохранены.

Все тесты workspace прошли. Запушено: `9462842`. [Замеры](../benches/results/2026-10-03-wal-zero-blocks/report.md).

На 1k выигрыш неустойчив; SQLite пока быстрее. Цель по восьми движкам остаётся открытой.

</details>

<details>
<summary>303. 2026-10-03T09:21:46.034Z</summary>

Продолжу сокращать стоимость кодирования WAL. Проверю, можно ли сохранить количество ненулевых значений при выборе sparse codec и использовать его при записи, убрав повторный проход по каждому embedding.

</details>

<details>
<summary>304. 2026-10-03T09:25:47.619Z</summary>

Кеширование счётчиков не проходит по замерам: на 10k есть небольшой выигрыш, но на 1k — замедление на 5,1%, проигрыш в 5 из 6 пар. Откатываю эксперимент; сохранённое ранее ускорение через нулевые блоки остаётся.

</details>

<details>
<summary>305. 2026-10-03T09:26:46.340Z</summary>

Кеширование счётчиков откатил: durable-вставка 1k замедлилась на **5,1%**, проиграв в 5/6 пар. Прежнее ускорение WAL сохранено.

[Результаты эксперимента](../benches/results/2026-10-03-wal-cached-counts/report.md) запушены: `fac9cd1`. Рабочее дерево чистое. Победа над всеми восемью движками пока не доказана.

</details>

<details>
<summary>306. 2026-10-03T09:27:01.624Z</summary>

Проверю общий путь bulk-вставки: какие проверки повторяются для каждой строки и какие из них можно подготовить один раз на batch, сохранив ограничения данных.

</details>

<details>
<summary>307. 2026-10-03T09:29:46.172Z</summary>

Профиль native-вставки показал, что формирование строк — самый дорогой из замеренных этапов: 0,543 мс на 1k и 5,678 мс на 10k. Это больше стоимости embedding. Проверка FK здесь — короткий проход без выделений памяти; основной кандидат для следующей оптимизации — сборка Row и её аллокации.

</details>

<details>
<summary>308. 2026-10-03T09:33:10.572Z</summary>

Свежий профиль показал главный участок для работы: формирование Row занимает **0,543 мс на 1k** и **5,678 мс на 10k** — больше, чем embedding.

[Замеры](../benches/results/2026-10-03-insert-phase-profile/report.md) запушены: `82a7b33`. Временная инструментализация удалена. Следующий шаг — измерить аллокации при сборке Row; победа над всеми восемью движками ещё не доказана.

</details>

<details>
<summary>309. 2026-10-03T09:34:16.765Z</summary>

Измерю число и объём аллокаций по этапам вставки — прежде всего при формировании Row. Счётчик будет только в диагностической сборке; её время не буду использовать для сравнений скорости.

</details>

<details>
<summary>310. 2026-10-03T09:40:39.299Z</summary>

Счётчик показал 16 003 аллокации при формировании 1k строк. Значительная часть — ключи Row и текстовые Cell. Следующим проверю разделяемые текстовые литералы в AST: это потенциально уберёт повторные аллокации значений при выполнении prepared insert и сохранит публичный тип Row.

</details>

<details>
<summary>311. 2026-10-03T09:44:28.534Z</summary>

Диагностические аллокации совпали во всех 9 вставках каждого размера. Собираю вариант с `Arc<str>` для текстовых литералов AST, чтобы подготовленные запросы разделяли текстовое содержимое с Cell. Публичный Row остаётся `BTreeMap<String, Cell>`.

</details>

<details>
<summary>312. 2026-10-03T09:46:54.130Z</summary>

Разделяемые литералы дали устойчивое ускорение: native-вставка быстрее на 5,3–5,6%, выигрыш во всех 6 парах для обоих размеров. Durable-вставка 10k ускорилась на 3,3% в 6/6 парах; на 1k эффект неустойчив. Проверю весь workspace и GPU-конфигурацию. Это также меняет тип литерала в экспериментальном AST на `Arc<str>`; стабильные Row и Cell сохраняются.

</details>

<details>
<summary>313. 2026-10-03T09:53:57.804Z</summary>

Workspace и GPU-сборка прошли. Дополнительные замеры чтения выявили компромисс: фильтры ускорились, а FTS по частому термину замедлился примерно на 2,2% в шести парах. Сохраню вариант за устойчивое ускорение вставки и фильтров; регрессию FTS явно отмечу в отчёте как оставшийся участок работы.

</details>

<details>
<summary>314. 2026-10-03T09:55:46.834Z</summary>

Запушил `b77b0e9`:

- Native-вставка быстрее на **5,3–5,6%**, победа в 6/6 пар.
- Durable-вставка 10k быстрее на **3,3%**, 6/6.
- Фильтры быстрее на **6,9–9,6%**.
- Компромисс: FTS частого термина медленнее на **2,2%**.

Workspace-тесты и GPU-сборка прошли. В экспериментальном AST литералы теперь `Arc<str>`; для `String` нужен `.into()`.

[Полные замеры](../benches/results/2026-10-03-shared-ast-strings/report.md). Цель по восьми движкам ещё не достигнута.

</details>

<details>
<summary>315. 2026-10-03T09:55:54.727Z</summary>

Проверю оставшуюся регрессию FTS: что происходит между получением кандидатов и выдачей top-k, и есть ли там лишние копии или проходы.

</details>

<details>
<summary>316. 2026-10-03T09:57:31.151Z</summary>

Нашёл лишнюю аллокацию: lexical scorer всегда создаёт lowercase-копию текста, даже когда ASCII уже в нижнем регистре. Проверю заимствование исходной строки для этого случая. Unicode останется на прежнем `str::to_lowercase()`, чтобы сохранить контекстное преобразование регистра.

</details>

<details>
<summary>317. 2026-10-03T09:58:00.114Z</summary>

Подготовил эксперимент: FTS scorer заимствует ASCII-текст, если он уже в нижнем регистре, вместо выделения lowercase-копии. Unicode обрабатывается прежним способом.

Тесты ещё идут. Ускорение пока не измерено; правку не пушил, цель остаётся открытой.

</details>

<details>
<summary>318. 2026-10-03T09:58:05.824Z</summary>

Проверю запущенные тесты и сборку FTS-варианта. После их завершения выполню парные замеры с текущим main.

</details>

<details>
<summary>319. 2026-10-03T09:59:04.742Z</summary>

FTS по частому термину ускорился на 8,2%, выиграв во всех 6 парах. Выборочный поиск тоже быстрее; на запросе без совпадений ускорения нет. Добавлю проверку Unicode-регистра и завершу проверку workspace.

</details>

<details>
<summary>320. 2026-10-03T10:00:34.091Z</summary>

Все тесты workspace прошли, включая проверку контекстного Unicode-регистра. Сохраню FTS-ускорение и запушу замеры. Общая цель остаётся открытой: SQLite всё ещё быстрее на малых и durable-вставках, MSSQL/Kusto ещё не проверены.

</details>

<details>
<summary>321. 2026-10-03T10:01:10.026Z</summary>

Ускорил FTS частого термина на **8,2%**, выигрыш в **6/6** парных запусков. Убрал лишнюю lowercase-аллокацию для ASCII; Unicode-семантика сохранена.

Все тесты workspace прошли. Запушено: `e71a17b`. [Замеры](../benches/results/2026-10-03-lex-borrow-lower/report.md).

Цель по восьми движкам ещё открыта: SQLite быстрее на части вставок, MSSQL/Kusto не проверены.

</details>

<details>
<summary>322. 2026-10-03T10:01:21.422Z</summary>

Обновлю общую картину после последних оптимизаций: прогоню одинаковые read-запросы против доступных движков и зафиксирую оставшиеся проигрыши. Для серверных баз использую отдельные benchmark-контейнеры.

</details>

<details>
<summary>323. 2026-10-03T10:06:18.977Z</summary>

Прогоны на 1k и 10k завершились. Общий gate пока отмечает их как incomplete: для MSSQL и Kusto не заданы подключения. Прогон на 100k продолжается; после него разберу результаты доступных движков по каждому запросу.

</details>

<details>
<summary>324. 2026-10-03T10:09:14.116Z</summary>

На всех трёх размерах Lin выиграл по агрегированным медианам во всех 36 доступных read-сравнениях — всего 108/108 против SQLite, DuckDB, PostgreSQL, MySQL, MongoDB и pandas. Контрольные данные не изменились, временных объектов не осталось, контейнеры остановлены. Сохраню матрицу с явными отметками о непроверенных MSSQL/Kusto.

</details>

<details>
<summary>325. 2026-10-03T10:10:42.114Z</summary>

Обновил read-бенчи на 1k, 10k и 100k строк: **108/108 побед по агрегированным медианам** против SQLite, DuckDB, PostgreSQL, MySQL, MongoDB и pandas.

[Полная матрица](../benches/results/2026-10-03-peer-read-refresh/report.md), запушено `7eaffc5`.

Цель ещё открыта: MSSQL/Kusto без подключений не проверены; проигрыши SQLite на части вставок остаются.

</details>

<details>
<summary>326. 2026-10-03T10:11:13.677Z</summary>

Продолжу работу над вставкой 1k, где Lin ещё уступает SQLite. Проверю FTS append-путь на повторяющихся токенах и сравню возможную правку с текущим main.

</details>

<details>
<summary>327. 2026-10-03T10:16:11.424Z</summary>

Проверяю хранение первого posting прямо внутри списка: редкому токену это может сэкономить отдельную heap-аллокацию. Формат FTS сохранён, проверки вставки, удаления, folding и codec прошли. Теперь сравню native- и durable-вставки.

</details>

<details>
<summary>328. 2026-10-03T10:29:16.332Z</summary>

Вариант с inline postings дал небольшой выигрыш на вставках: +2,34% для 1 000 строк и +1,38% для 10 000. На чтении результат нестабилен, а durable-вставка 1 000 строк не ускорилась. Откатываю этот эксперимент и сохраняю замеры: данных недостаточно, чтобы оставлять усложнение в коде.

</details>

<details>
<summary>329. 2026-10-03T10:30:15.003Z</summary>

Проверил inline postings: вставки ускорились на 1,4–2,3%, но чтение нестабильно, durable 1k без выигрыша. Вариант откатил.

[Замеры и выводы](../benches/results/2026-10-03-fts-inline-postings/report.md) запушены в `main`, коммит `73e2f18`. Рабочее дерево чистое.

SQLite пока быстрее на вставках 1k и durable-вставках — задача ещё не закрыта.

</details>

<details>
<summary>330. 2026-10-03T10:30:22.365Z</summary>

Следующая проверка — стоимость хеширования биграмм при построении embedding. Проверю, можно ли заменить повторные вычисления таблицей индексов, сохранив точные значения векторов.

</details>

<details>
<summary>331. 2026-10-03T10:31:47.042Z</summary>

Таблица совпала с исходным хешированием для всех 65 536 пар байтов; тесты векторов прошли побитно. Цена варианта — 128 КиБ памяти и одно вычисление таблицы при создании первого embedder. Теперь измерю вставки, чтобы проверить, окупается ли это.

</details>

<details>
<summary>332. 2026-10-03T10:33:22.350Z</summary>

В шести парных прогонах вставка 10 000 строк ускорилась на 2,48%, durable-вставка — на 3,28% с выигрышем во всех шести парах. Результат для 1 000 строк слабее: заметно сдвинулся и SQLite-контроль. Проверю полный набор тестов и отдельно измерю стоимость первой инициализации.

</details>

<details>
<summary>333. 2026-10-03T10:34:54.200Z</summary>

Полный набор тестов прошёл. Первая инициализация занимает около 73 мкс, последующие конструкторы — десятки наносекунд; hybrid-поиск в этих прогонах почти не изменился. Оставляю оптимизацию ради выигрыша на больших вставках. SQLite всё ещё быстрее на малых и durable-вставках.

</details>

<details>
<summary>334. 2026-10-03T10:35:44.320Z</summary>

Оптимизация embedding запушена в `main`: `2651115`.

По шести парным прогонам:

- Вставка 10 000 строк: быстрее на **2,48%**.
- Durable-вставка 10 000: **3,28%**, выигрыш 6/6.
- Векторы совпадают побитно; полный набор тестов прошёл.

Цена: 128 КиБ памяти и около 73 мкс первой инициализации. [Отчёт и замеры](../benches/results/2026-10-03-embed-bigram-table/report.md).

SQLite пока быстрее на малых и durable-вставках; MSSQL и Kusto ещё не проверены. Цель остаётся открытой.

</details>

<details>
<summary>335. 2026-10-03T10:35:48.283Z</summary>

Продолжаю с durable-вставками: проверю текущую сериализацию WAL и найду, где ещё можно убрать работу без изменения формата и гарантий записи.

</details>

<details>
<summary>336. 2026-10-03T10:36:55.116Z</summary>

Проверяю маску ненулевых элементов для каждого блока WAL: после её построения можно записывать только присутствующие значения. Формат записи сохраняется; проверки `-0.0`, NaN, границ блоков, неполного хвоста и повреждённых данных прошли.

</details>

<details>
<summary>337. 2026-10-03T10:38:14.858Z</summary>

Прогон пока не дошёл до измерений: процесс жив, но не выдаёт даже стартовый отчёт. Проверяю, что удерживает запуск; повторную копию бенча не запускаю.

</details>

<details>
<summary>338. 2026-10-03T10:40:21.700Z</summary>

Маска не окупилась: durable-вставка 1 000 строк стала медленнее на 1,69%, а выигрыш для 10 000 составил лишь 0,65% — меньше сдвига SQLite-контроля. Откатываю вариант и сохраняю результаты.

</details>

<details>
<summary>339. 2026-10-03T10:41:15.640Z</summary>

Проверил ещё один вариант сериализации WAL в шести парных прогонах:

- Durable 1 000 строк: **медленнее на 1,69%**.
- Durable 10 000: быстрее на 0,65%, слабее сдвига SQLite-контроля.

Вариант откатил. [Замеры](../benches/results/2026-10-03-wal-block-mask/report.md) запушены в `main`, `ff5fc07`; рабочее дерево чистое.

Разрыв с SQLite на durable-вставках остаётся. Полная цель пока не достигнута.

</details>

<details>
<summary>340. 2026-10-03T18:53:03.997Z</summary>

Проверю, не считаем ли объём sparse WAL дольше необходимого: выбор codec может прекращаться, как только sparse уже не меньше dense. Это сохраняет формат и решение, но может убрать лишний обход плотных векторов.

</details>

<details>
<summary>341. 2026-10-03T18:53:51.709Z</summary>

Ранний выход при выборе codec поможет плотным векторам, но не текущей sparse-нагрузке, где остаётся разрыв с SQLite. Проверю более существенный участок: построение строк больших batches на двух потоках. Выделение ID и проверки уникальности/FK останутся последовательными.

</details>

<details>
<summary>342. 2026-10-03T18:55:49.404Z</summary>

Вариант запускает один дополнительный поток для batches от 4 096 строк; создание и завершение потока входят во время вставки. Сравню и 1 000, и 10 000 строк: предварительное построение rows может повлиять на кэш и малые batches, даже без второго потока.

</details>

<details>
<summary>343. 2026-10-03T18:56:40.533Z</summary>

На 10 000 строках есть устойчивый выигрыш: native +2,82%, durable +5,37%, в обоих случаях 6/6 пар. На native 1 000 строк Lin и SQLite замедлились примерно одинаково. Перед сохранением проверю порядок rows и ID, общий `now` и ошибки уникальности на границе потоков.

</details>

<details>
<summary>344. 2026-10-03T18:58:28.288Z</summary>

Проверки порядка, ID, общего `now` и дубликатов между половинами batch прошли, как и полный набор тестов. Сохраняю вариант: он сокращает время больших вставок ценой второго CPU-потока и временного массива row headers. Разрыв с SQLite на durable-вставке 10 000 строк остаётся.

</details>

<details>
<summary>345. 2026-10-03T18:59:23.955Z</summary>

Запушил `156e991`: batches от 4 096 строк строят rows на двух потоках.

Шесть парных прогонов для 10 000 строк:

- Native-вставка: **+2,82%**, выигрыш 6/6.
- Durable-вставка: **+5,37%**, выигрыш 6/6.

Полный набор тестов прошёл. Цена — дополнительный поток и временная память. [Отчёт](../benches/results/2026-10-03-parallel-row-build/report.md).

SQLite пока быстрее на малых и durable-вставках. Полная цель остаётся открытой.

</details>

<details>
<summary>346. 2026-10-04T06:10:58.953Z</summary>

Проверю поведение новой оптимизации при нехватке ресурсов для потока. Большая вставка должна продолжаться последовательно, если ОС не смогла создать worker; затем проверю, сохраняется ли измеренный выигрыш.

</details>

<details>
<summary>347. 2026-10-04T06:16:15.456Z</summary>

Исправил отказ при создании worker: большая вставка теперь продолжается последовательно вместо panic.

Тест fallback и полный набор тестов прошли. Шесть парных прогонов не показали устойчивого замедления. [Замеры](../benches/results/2026-10-04-row-worker-fallback/report.md).

Запушено в `main`: `d70fed1`. Разрыв с SQLite на малых и durable-вставках остаётся; полная цель ещё не достигнута.

</details>

<details>
<summary>348. 2026-10-04T07:48:23.355Z</summary>

Проверю, окупается ли параллельное построение rows уже на 1 000 строках. Снижу порог только в эксперименте и сравню native и durable вставки; оставлю изменение лишь при подтверждённом выигрыше.

</details>

<details>
<summary>349. 2026-10-04T07:49:52.561Z</summary>

Native-прогоны завершились, durable ещё идут. Создание и завершение потока входят в измеряемое время: для малого batch именно эта стоимость может съесть выигрыш.

</details>

<details>
<summary>350. 2026-10-04T07:50:32.115Z</summary>

Низкий порог не дал выигрыша: native-вставка 1 000 строк медленнее на 0,51%, durable — на 1,64%; быстрее только 2 из 6 пар в обоих случаях. Возвращаю порог 4 096 и сохраняю замеры.

</details>

<details>
<summary>351. 2026-10-04T07:51:10.812Z</summary>

Проверил второй поток на batch из 1 000 строк: выигрыша нет — native медленнее на 0,51%, durable на 1,64%.

Эксперимент откатил, порог остался 4 096. [Замеры](../benches/results/2026-10-04-row-worker-low-threshold/report.md) запушены в `main`: `703bac5`.

Разрыв с SQLite сохраняется; полная цель ещё не достигнута.

</details>

<details>
<summary>352. 2026-10-04T16:14:05.991Z</summary>

Обновлю профиль вставки на текущем коде: после изменений в rows и embedding старые доли затрат могли сдвинуться. Это позволит выбрать следующий участок по измерениям.

</details>

<details>
<summary>353. 2026-10-04T16:17:23.686Z</summary>

Сейчас крупнейшая измеренная фаза — embedding: около 0,30 мс на 1 000 строк и 3,44 мс на 10 000. Построение rows и проверки занимают меньше. Следующим проверю стоимость хеширования триграмм; таймеры убраны из рабочего кода, профиль сохраню отдельно.

</details>

<details>
<summary>354. 2026-10-04T16:18:16.688Z</summary>

Обновил профиль на текущем коде. Крупнейшая измеренная фаза — embedding: **0,30 мс для 1 000 строк**, **3,44 мс для 10 000**.

Следующий участок для проверки — повторное хеширование триграмм. [Профиль и сырые результаты](../benches/results/2026-10-04-current-insert-profile/report.md) запушены: `5a6937b`.

Рабочее дерево чистое. Это диагностика, нового ускорения пока нет; полная цель остаётся открытой.

</details>

<details>
<summary>355. 2026-10-04T16:23:02.342Z</summary>

Проверю таблицу индексов для ASCII-триграмм в default embedding. Сначала подтвержу побитное совпадение с исходным хешированием, затем измерю вставки. Unicode и остальные размерности сохранят прежний путь.

</details>

<details>
<summary>356. 2026-10-04T16:25:12.656Z</summary>

Все 17 576 тройки букв совпали с исходным хешированием; семь embedding-тестов прошли, включая побитное сравнение векторов. Дополнительная таблица занимает около 34 КиБ. Проверяю, даёт ли lookup выигрыш с учётом проверок диапазона и обращений к памяти.

</details>

<details>
<summary>357. 2026-10-04T16:26:04.623Z</summary>

Вариант замедлил native-вставки: на 3,53% для 1 000 строк и 4,38% для 10 000. Durable-результат лучше, но там ускорился и SQLite-контроль. Таблицу триграмм откатываю: выигрыш не подтверждён.

</details>

<details>
<summary>358. 2026-10-04T16:26:52.329Z</summary>

Таблица триграмм не окупилась: native-вставки замедлились на **3,53–4,38%** в шести парных прогонах. Вариант откатил.

[Замеры](../benches/results/2026-10-04-embed-trigram-table/report.md) запушены в `main`: `3f5e556`. Рабочее дерево чистое.

Разрыв с SQLite остаётся; полная цель ещё не достигнута.

</details>

<details>
<summary>359. 2026-10-04T16:26:59.368Z</summary>

Проверю упаковку пары «индекс + значение» в sparse WAL: сейчас она записывается двумя отдельными вызовами. Объединение в один может снизить стоимость сериализации, сохранив те же байты и гарантии записи.

</details>

<details>
<summary>360. 2026-10-04T16:28:48.893Z</summary>

Проверки sparse WAL прошли: сохраняются биты `-0.0`, NaN и Inf, границы блоков и неполный хвост. Сравниваю шесть пар fresh-fixture прогонов; fsync и остальные условия durable-вставки прежние.

</details>

<details>
<summary>361. 2026-10-04T16:30:30.050Z</summary>

Остались последние пары прогонов. Сравню результат со сдвигом SQLite-контроля: ускорение обоих вариантов может отражать изменение нагрузки на машине.

</details>

<details>
<summary>362. 2026-10-04T16:31:44.021Z</summary>

Получился сильный общий сдвиг: на 10 000 строк candidate Lin медленнее на 20,6%, но неизменённый SQLite-контроль — на 22,7%. По этому прогону нельзя уверенно судить об изменении WAL. Повторю крупную вставку отдельно.

</details>

<details>
<summary>363. 2026-10-04T16:32:59.421Z</summary>

Отдельный повтор дал +2,49% для Lin, быстрее в 5 из 6 пар; SQLite-контроль при этом замедлился на 1,85%. Проверю повтором и 1 000 строк: исходный общий сдвиг затронул обе размерности.

</details>

<details>
<summary>364. 2026-10-04T16:34:23.600Z</summary>

Повтор для 1 000 строк показал замедление Lin на 2,47%, быстрее только 1 из 6 пар; SQLite-контроль почти не изменился. Выигрыш на крупном batch не компенсирует ухудшение малого. Откатываю вариант и сохраняю все три серии замеров.

</details>

<details>
<summary>365. 2026-10-04T16:35:15.614Z</summary>

Проверил объединённую запись пары WAL. В отдельных повторах:

- 10 000 строк: **+2,49%**, быстрее 5/6 пар.
- 1 000 строк: **−2,47%**, быстрее лишь 1/6 при стабильном SQLite-контроле.

Вариант откатил. [Все замеры](../benches/results/2026-10-04-wal-paired-write/report.md) запушены: `2182950`.

Разрыв с SQLite остаётся; полная цель ещё не достигнута.

</details>

<details>
<summary>366. 2026-10-04T16:35:58.974Z</summary>

Проверю ASCII-разбиение слов в embedding. Для обычного текста оно может быть дешевле Unicode-разбиения, но должно сохранить семантику whitespace, включая вертикальную табуляцию.

</details>

<details>
<summary>367. 2026-10-04T16:38:23.644Z</summary>

Побитное сравнение прошло для всех 128 ASCII-символов внутри текста и набора Unicode-разделителей. Вертикальная табуляция отправляется на прежний путь. Теперь измеряю вставки, включая стоимость этих дополнительных проверок.

</details>

<details>
<summary>368. 2026-10-04T16:39:58.550Z</summary>

На native-вставках есть выигрыш во всех шести парах: +13,83% для 1 000 строк и +5,30% для 10 000. SQLite-контроль тоже ускорился, но меньше. Durable-результаты смешанные; проверю полный набор тестов и отдельно повторю малый native batch.

</details>

<details>
<summary>369. 2026-10-04T16:41:57.060Z</summary>

Отдельный повтор подтвердил выигрыш на native 1 000 строк: +6,16%, быстрее во всех шести парах, при почти стабильном SQLite-контроле. Полный набор тестов прошёл. Сохраняю оптимизацию; SQLite на этом batch всё ещё быстрее примерно на 4%.

</details>

<details>
<summary>370. 2026-10-04T16:43:08.333Z</summary>

Ускорил разбиение ASCII-текста в embedding, сохранив побитные значения векторов.

Отдельный повтор native-вставки 1 000 строк: **+6,16%**, выигрыш 6/6 пар. Полный набор тестов прошёл.

Запушено в `main`: `0041eef`. [Замеры](../benches/results/2026-10-04-embed-ascii-split/report.md).

SQLite на малом batch всё ещё быстрее примерно на 4%; durable-выигрыш не подтверждён. Полная цель остаётся открытой.

</details>

<details>
<summary>371. 2026-10-04T16:45:13.604Z</summary>

Побитные проверки embedding прошли, включая ASCII-символы, Unicode-разделители и разные размерности. Дополнительной таблицы или heap-аллокации нет. Сравниваю native и durable вставки на 1 000 и 10 000 строках.

</details>

<details>
<summary>372. 2026-10-04T16:46:52.369Z</summary>

Первый результат слабый: native 1 000 строк быстрее на 0,91% в 5 из 6 пар, а на 10 000 почти без изменения. Повторю малый native batch отдельно, чтобы понять, устойчив ли этот небольшой выигрыш.

</details>

<details>
<summary>373. 2026-10-04T16:47:31.068Z</summary>

Повтор не подтвердил выигрыш: Lin −0,23%, SQLite-контроль −0,26%, быстрее только 2 из 6 пар в обоих случаях. Возвращаю прежнее хеширование и сохраняю результаты.

</details>

<details>
<summary>374. 2026-10-04T16:48:14.366Z</summary>

Фиксированный массив для триграмм не дал подтверждённого ускорения: повтор показал **−0,23%**, почти как SQLite-контроль.

Вариант откатил. [Замеры](../benches/results/2026-10-04-embed-fixed-trigrams/report.md) запушены: `172703c`. Рабочее дерево чистое.

Предыдущее ASCII-ускорение сохранено; полная цель ещё не достигнута.

</details>

<details>
<summary>375. 2026-10-04T16:49:33.289Z</summary>

Для durable-разрыва проверю потенциал более компактного WAL: сколько различных f32-значений реально содержат векторы default embedding. Словарь битовых значений может уменьшить записи без потери точности. Сначала измерю объём, формат пока не меняю.

</details>

<details>
<summary>376. 2026-10-04T16:53:47.037Z</summary>

Потенциал есть: в каждом из 11 000 векторов встретилось только 4–6 разных ненулевых f32-значений. Прототип словаря уменьшил объём vector-части с 239 до 113 КБ для 1 000 строк и с 2,54 до 1,18 МБ для 10 000. Побитный roundtrip прошёл в трёх процессах. Это замер размера, ускорение ещё не проверено.

</details>

<details>
<summary>377. 2026-10-04T16:55:07.670Z</summary>

Нашёл потенциал для сокращения WAL: словарь точных f32-битов уменьшает **vector-часть на 53%**. Проверка 11 000 векторов в трёх процессах прошла побитно.

Это замер размера, не ускорения; рабочий формат пока не менял. [Аудит](../benches/results/2026-10-04-vector-dictionary-audit/report.md) запушен: `f0d424d`.

Следующий шаг — совместимый codec и durable-бенч. Полная цель остаётся открытой.

</details>

<details>
<summary>378. 2026-10-04T16:55:18.856Z</summary>

Перехожу к экспериментальному dictionary-codec. Он будет хранить точные f32-биты и читать старые записи; для неповторяющихся значений нужен raw fallback. Проверю границы декодера, reopen и только затем durable-бенч.

</details>

<details>
<summary>379. 2026-10-04T17:03:50.811Z</summary>

Новые проверки codec прошли. В полном suite остановился тест, который ожидал старый номер codec 3 для большой вставки; теперь запись использует 4. Обновлю это ожидание и повторю suite, сохранив проверки количества frames, reopen и точных векторов.

</details>

<details>
<summary>380. 2026-10-04T17:06:20.990Z</summary>

Полный suite прошёл, включая большую вставку в один frame, reopen и WAL replay. Проверены границы индексов 65 536/65 537, переполнение словаря, неверные value-codes и повреждённые данные. Перехожу к сравнению latency: меньший WAL может обойтись дороже по CPU.

</details>

<details>
<summary>381. 2026-10-04T17:08:03.469Z</summary>

Codec уменьшает vector-часть, но этот encoder заметно замедлил durable-вставки: на 20,66% для 1 000 строк и 32,60% для 10 000, проигрыш во всех шести парах при стабильном SQLite-контроле. Откатываю формат и сохраняю прототип, тесты и замеры для разбора стоимости кодирования.

</details>

<details>
<summary>382. 2026-10-04T17:09:35.132Z</summary>

Реализовал и проверил dictionary-codec. Тесты битовой точности, границ, reopen и replay прошли, но durable-вставки замедлились на **20,66–32,60%**, проиграв все 6 пар при стабильном SQLite-контроле.

Формат откатил. [Прототип, тесты и замеры](../benches/results/2026-10-04-vector-dictionary-codec/report.md) запушены: `0b31508`.

Меньшего размера оказалось недостаточно — нужно снижать стоимость кодирования. Полная цель остаётся открытой.

</details>

<details>
<summary>383. 2026-10-04T17:10:17.631Z</summary>

Проверю другой encoder для того же dictionary-прототипа: один поиск value-code на ненулевой элемент, пропуск нулевых блоков и запись из небольшого буфера координат. Это убирает часть повторных обходов, которые могли съесть выигрыш от меньшего WAL.

</details>

<details>
<summary>384. 2026-10-04T17:13:16.689Z</summary>

Восемь WAL integrity-тестов прошли с буферным encoder. Default-векторы должны помещаться в inline-буфер на 64 координаты; более насыщенные требуют временной heap-памяти. Сравниваю вариант с текущим codec 3, чтобы проверить весь tradeoff по времени вставки.

</details>

<details>
<summary>385. 2026-10-04T17:15:12.619Z</summary>

Буферный вариант тоже проиграл текущему codec 3: durable 1 000 строк медленнее на 11,85%, 10 000 — на 15,38%, во всех шести парах. SQLite-контроль немного ускорился. Откатываю прототип; дополнительная обработка всё ещё не окупается по latency.

</details>

<details>
<summary>386. 2026-10-04T17:16:07.368Z</summary>

Буферный dictionary-encoder тоже не окупился: durable-вставки медленнее текущего codec на **11,85–15,38%**, проигрыш 6/6 пар при стабильном SQLite-контроле.

Вариант откатил. [Прототип и замеры](../benches/results/2026-10-04-vector-dictionary-buffered/report.md) запушены: `1a2dc09`.

Следующий вариант — меньшая ширина координат без словаря и дополнительного буфера. Полная цель остаётся открытой.

</details>

<details>
<summary>387. 2026-10-04T17:16:47.350Z</summary>

Проверю более простой codec: координаты sparse-векторов в u16 при размерности до 65 536, f32-биты без изменений; выше границы — u32. Словаря и буфера нет. Проверю чтение старых записей, граничные индексы, reopen и durable latency.

</details>

<details>
<summary>388. 2026-10-04T17:22:32.764Z</summary>

Первый прогон дал +8,13% на durable 1 000 строк, быстрее в 5 из 6 пар при почти неизменном SQLite-контроле. На 10 000 выигрыш слабее — 0,94%. Повторю малый batch отдельно и проверю полный suite, прежде чем сохранять новый формат.

</details>

<details>
<summary>389. 2026-10-04T17:26:14.604Z</summary>

Повтор дал лишь +1,28% для Lin, быстрее в 4 из 6 пар; SQLite-контроль ускорился сильнее — на 2,25%. Первый выигрыш не подтвердился достаточно убедительно. Откатываю codec 5: новый формат пока не оправдан измеренной latency.

</details>

<details>
<summary>390. 2026-10-04T17:27:18.995Z</summary>

Codec с u16-координатами прошёл полный suite, но ускорение не подтвердилось повтором: Lin **+1,28%**, SQLite-контроль **+2,25%**.

Формат откатил. [Прототип и замеры](../benches/results/2026-10-04-wal-narrow-coordinates/report.md) запушены: `525b257`.

Рабочее дерево чистое. Разрыв с SQLite остаётся; полная цель ещё не достигнута.

</details>

<details>
<summary>391. 2026-10-04T17:27:36.735Z</summary>

Вернусь к упаковке rows в WAL: проверю, сколько обходов строк и аллокаций происходит до encoder. Если там есть лишняя работа, её можно убрать без изменения формата.

</details>

<details>
<summary>392. 2026-10-04T17:36:37.091Z</summary>

Поправил setup байтового теста: helper дописывал второй frame в тот же файл вместо независимого сравнения. После сброса файла проверю точные WAL-байты, включая числовые преобразования, nulls, `-0.0` и NaN.

</details>

<details>
<summary>393. 2026-10-04T17:39:50.964Z</summary>

Первый прогон дал слабый эффект: на 10 000 строк +1,29% в 5 из 6 пар, на 1 000 — почти без изменения. Повторю обе размерности: одного такого результата недостаточно, чтобы оставлять отдельный fast path.

</details>

<details>
<summary>394. 2026-10-04T17:43:39.359Z</summary>

Повтор подтвердил выигрыш только на 10 000 строках: +3,14%, быстрее 6/6 пар; малый batch снова слегка хуже. Ограничу fast path большими batches и повторно проверю обе размерности, чтобы малые использовали прежнюю упаковку.

</details>

<details>
<summary>395. 2026-10-04T17:49:28.803Z</summary>

Полный набор тестов прошёл, включая проверку побайтовой совместимости WAL на 4096 строках. Запускаю финальный замер: новая упаковка включена только для больших batches.

</details>

<details>
<summary>396. 2026-10-04T17:50:32.267Z</summary>

Финальный вариант ускорил durable-вставку 10 000 строк на 1,89% — выиграл все 6 пар замеров. На 1000 строк результат в пределах шума: +0,09%, 3/6. Сохраняю оптимизацию и результаты бенча; SQLite на durable-вставке пока быстрее.

</details>

<details>
<summary>397. 2026-10-04T17:51:38.362Z</summary>

Оптимизировал упаковку WAL для больших batches, запушил в `main`: `1edcdca`.

- Durable-вставка 10 000 строк: **+1,89%, выигрыш 6/6 пар**.
- 1000 строк: без значимого изменения.
- Все тесты прошли, формат WAL сохранён.

[Результаты бенча](../benches/results/2026-10-04-row-major-wal-pack/report.md). SQLite на durable-вставке пока быстрее.

</details>

<details>
<summary>398. 2026-10-04T17:51:44.663Z</summary>

Предыдущий этап дал подтверждённое ускорение больших durable-вставок. Дальше проверю, где сейчас тратится время упаковки WAL: разрыв с SQLite ещё заметный, и следующий шаг выберу по текущему коду и замерам.

</details>

<details>
<summary>399. 2026-10-04T17:52:25.245Z</summary>

В записи каждого WAL-frame сейчас четыре вызова `write_all`: заголовок, кодек, payload и checksum. Проверю запись этих частей через `write_vectored`, сохранив те же байты и flush. Отдельно проверю короткие записи, `Interrupted` и ошибку записи нулевой длины.

</details>

<details>
<summary>400. 2026-10-04T17:54:11.699Z</summary>

Все 8 WAL-тестов прошли, включая короткие записи и обработку ошибок. Теперь идут парные durable-замеры на 1000 и 10 000 строках; SQLite служит контролем, чтобы отличить эффект изменения от колебаний машины.

</details>

<details>
<summary>401. 2026-10-04T17:54:42.711Z</summary>

`write_vectored` не дал устойчивого ускорения: на 1000 строках +1,02%, но лишь 3/6 побед; на 10 000 — −0,06%, тоже 3/6. Откатываю этот вариант и сохраняю замеры. Значимого выигрыша от объединения этих записей здесь не видно.

</details>

<details>
<summary>402. 2026-10-04T17:55:25.859Z</summary>

Проверил объединение WAL-записей через `write_vectored`: устойчивого ускорения нет, обе размерности выиграли лишь 3/6 пар. Вариант откатил; 8 WAL-тестов прошли.

[Замеры и отчёт](../benches/results/2026-10-04-wal-vectored-write/report.md) запушены: `c21d893`. Предыдущее ускорение сохранено. Цель обогнать всех перечисленных конкурентов ещё не достигнута.

</details>

<details>
<summary>403. 2026-10-04T18:02:20.544Z</summary>

Продолжаю: обновлю профиль durable-вставки после последних оптимизаций. Это покажет, что сейчас даёт основной вклад — построение строк, embedding, упаковка или кодирование WAL — и поможет выбрать следующий эксперимент.

</details>

<details>
<summary>404. 2026-10-04T18:05:58.957Z</summary>

В свежем профиле 10 000 строк кодирование WAL занимает около 6,88 мс, embedding — 4,02 мс, упаковка — 2,24 мс; сами записи файла — 0,29 мс. Это диагностические замеры, не сравнение движков. Уточню вклад отдельных колонок, чтобы выбрать следующий эксперимент.

</details>

<details>
<summary>405. 2026-10-04T18:07:35.112Z</summary>

Уточнение по колонкам: в WAL для 10 000 строк `embedding` занимает около 6,61 мс, а самая дорогая текстовая колонка — 0,10 мс. Классификация sparse/dense входит в эти 6,61 мс и занимает около 1,29 мс. Следующий эксперимент направлю на sparse-векторы, сохранив точные float-биты, формат и проверки replay.

</details>

<details>
<summary>406. 2026-10-04T18:08:48.052Z</summary>

Обновил профиль: на 10 000 строк кодирование `embedding` в WAL занимает **6,61 мс** — крупнейшая измеренная фаза. Следующий эксперимент — ускорение sparse-векторов с сохранением точных байтов.

[Профиль и сырые замеры](../benches/results/2026-10-04-durable-phase-refresh/report.md) запушены: `73bd87a`. Рабочий код восстановлен, временные измерители убраны. Превосходство над всеми конкурентами пока не доказано.

</details>

<details>
<summary>407. 2026-10-04T18:09:24.466Z</summary>

Свежий профиль указал на кодирование sparse-векторов. Проверю обработку большого batch в двух потоках: первый пишет левую половину в основной буфер, второй готовит правую, затем результаты соединяются в исходном порядке. Замер учтёт создание потока и дополнительный буфер.

</details>

<details>
<summary>408. 2026-10-04T18:12:17.565Z</summary>

Проверки прошли: параллельная упаковка совпадает с независимым последовательным эталоном побайтово, включая `None`, пустые векторы, отрицательный ноль и NaN. Проверен и последовательный fallback при отказе создания потока. Бенч запускает этот путь только для достаточно больших колонок.

</details>

<details>
<summary>409. 2026-10-04T18:13:53.264Z</summary>

Первый вариант дал +12,43% на 10 000 строках, но замедлил 1000 строк на 6,31%. Сохранять его целиком нельзя. Верну малому batch прежний цикл кодирования и повторю оба замера; параллельная ветка останется только для больших колонок.

</details>

<details>
<summary>410. 2026-10-04T18:15:40.311Z</summary>

Полный набор тестов прошёл для варианта с прежним малым циклом и отдельной параллельной веткой. Проверка реального WAL-frame также совпала с побайтовым эталоном. Финальный замер покажет, исчезла ли регрессия на 1000 строках.

</details>

<details>
<summary>411. 2026-10-04T18:16:58.615Z</summary>

Финальный вариант ускорил 10 000 строк на 11,14%, выиграв 6/6 пар. На 1000 строках прежняя заметная регрессия исчезла; текущий результат −0,42% близок к шуму. Проверю общий лимит декодирования для нескольких колонок и повторю замеры.

</details>

<details>
<summary>412. 2026-10-04T18:19:05.723Z</summary>

Повтор подтвердил ускорение большого batch: +11,00%, снова 6/6 побед. На 1000 строках результат +0,43%, без устойчивой регрессии. Проверка общего лимита 64 MiB прошла и подтвердила, что при ошибке существующий WAL не меняется. Сохраняю оптимизацию.

</details>

<details>
<summary>413. 2026-10-04T18:20:28.609Z</summary>

Запушил оптимизацию sparse-WAL в `main`: `2feb9a8`.

- 10 000 строк: **ускорение ≈11%**, два замера по 6/6 побед.
- 1000 строк: устойчивой регрессии нет.
- Полный набор тестов и побайтовые проверки прошли.

Цена — дополнительный поток и буфер. SQLite пока быстрее: 19,05 против 21,26 мс.

[Бенч и отчёт](../benches/results/2026-10-04-parallel-sparse-wal/report.md).

</details>

<details>
<summary>414. 2026-10-04T18:20:37.128Z</summary>

Продолжаю проверку оставшегося разрыва. Сначала посмотрю текущий путь batch-embedding и результаты прежних экспериментов, чтобы выбрать следующий замер без изменения семантики данных.

</details>

<details>
<summary>415. 2026-10-04T18:21:29.693Z</summary>

Ранее Rayon-пул не показал устойчивого выигрыша. Проверю другой вариант: один scoped-поток внутри `HashingEmbedder` только для больших batches. Малый batch сохранит прежний цикл, а сторонние embedders — свой batch-метод. Замерю и native-, и durable-вставку.

</details>

<details>
<summary>416. 2026-10-04T18:23:35.581Z</summary>

Семь embedding-тестов прошли. Проверка на границах 4095/4096/4101 строк дала те же float-биты и порядок, включая Unicode, пустые тексты и fallback при отказе создания потока. Идут парные замеры native- и durable-вставки.

</details>

<details>
<summary>417. 2026-10-04T18:27:25.172Z</summary>

Вариант с отдельной параллельной функцией также прошёл все семь embedding-тестов. Сейчас замеряется полная вставка, включая создание потока, подготовку текстов и объединение результатов.

</details>

<details>
<summary>418. 2026-10-04T18:28:38.741Z</summary>

Отдельная функция не помогла: native- и durable-вставка всё равно замедлились примерно на 5–8%. Откатываю оба варианта batch-параллелизма и сохраняю результаты. Подтверждённое ускорение sparse-WAL остаётся в `main`.

</details>

<details>
<summary>419. 2026-10-04T18:29:39.606Z</summary>

Проверил два варианта параллельного batch-embedding: оба замедлили вставку. Откатил их, предыдущее ускорение WAL сохранено.

[Бенчи и отчёт](../benches/results/2026-10-04-scoped-batch-embed/report.md) запушены: `bab6848`. Цель обогнать всех конкурентов остаётся открытой.

</details>

<details>
<summary>420. 2026-10-04T18:29:44.524Z</summary>

Продолжаю с кодированием WAL: проверю четыре ограниченных потока вместо двух для больших sparse-колонок. Малые batches сохранят прежний путь; решение приму по замерам полной вставки и проверке точных байтов.

</details>

<details>
<summary>421. 2026-10-04T18:31:24.592Z</summary>

Все девять WAL-тестов прошли для четырёх частей: порядок и байты сохранены, общий лимит декодирования и fallback работают. Теперь сравниваю этот вариант с сохранённым двухпоточным; дополнительные потоки и буферы входят в измеряемую операцию.

</details>

<details>
<summary>422. 2026-10-04T18:32:47.246Z</summary>

Первый замер четырёхпоточного варианта дал +8,00% на 10 000 строках, 6/6 побед; SQLite-контроль почти не изменился. Малый batch тоже оказался быстрее, хотя потоки в нём не создаются — причинного вывода здесь нет. Проверю полный набор тестов и повторю замеры.

</details>

<details>
<summary>423. 2026-10-04T18:35:08.962Z</summary>

Полный набор тестов прошёл. В первом замере Lin на 10 000 строках приблизился к SQLite: 19,47 против 18,94 мс, но пока остался медленнее. Повторный замер идёт с теми же зафиксированными бинарниками.

</details>

<details>
<summary>424. 2026-10-04T18:36:23.016Z</summary>

В повторе Lin выиграл 5/6 пар на 10 000 строках с медианным парным улучшением 8,28%. При этом SQLite-контроль тоже ускорился на 4,19%, поэтому весь эффект повторного замера нельзя приписать изменению. Сохраняю вариант с этим ограничением в отчёте: обе серии положительные, проверки прошли.

</details>

<details>
<summary>425. 2026-10-04T18:37:29.010Z</summary>

Запушил следующий шаг: `f1b5219`.

Большие sparse-колонки WAL кодируются в четыре потока. На 10 000 строках наблюдаемое ускорение ≈8%, победы 6/6 и 5/6; повтор также показал ускорение SQLite-контроля. Все тесты прошли, байты WAL сохранены.

[Замеры и ограничения](../benches/results/2026-10-04-four-way-sparse-wal/report.md). SQLite пока быстрее; общая цель ещё не достигнута.

</details>

<details>
<summary>426. 2026-10-04T18:37:59.681Z</summary>

Продолжаю сокращать разрыв на durable-вставке. Проверю параллельный подсчёт sparse-размера для больших векторных колонок: в свежем профиле эта классификация занимала около 1,3 мс. Малые колонки сохранят прежний путь, а замер учтёт создание дополнительных потоков.

</details>

<details>
<summary>427. 2026-10-04T18:40:24.513Z</summary>

Все десять WAL-тестов прошли. Новая проверка подтвердила точный sparse-размер и выбор формата для sparse/dense данных, отрицательного нуля, NaN, пустых векторов и отказа создания потока. Идут парные durable-замеры на обеих размерностях.

</details>

<details>
<summary>428. 2026-10-04T18:41:11.948Z</summary>

Первый замер дал +4,73% на 10 000 строках, 6/6 побед против прежнего Lin. В этой серии Lin впервые оказался чуть быстрее SQLite: 18,73 против 18,96 мс, 5/6 пар. Запас небольшой — проверяю полный набор тестов и повторяемость. На 1000 строках разрыв остаётся.

</details>

<details>
<summary>429. 2026-10-04T18:43:48.460Z</summary>

Повтор подтвердил результат: +3,89% на 10 000 строках, 5/6 побед против прежнего Lin при почти неизменном SQLite-контроле. В обеих сериях Lin также чуть быстрее SQLite на этом кейсе, по 5/6 пар. Сохраняю оптимизацию; это ещё не победа на малой вставке или над всем набором конкурентов.

</details>

<details>
<summary>430. 2026-10-04T18:45:07.456Z</summary>

Запушил `4926d9c`: параллельная классификация sparse-WAL дала ещё **+3,9–4,7%** на 10 000 строках.

В обеих сериях Lin чуть быстрее SQLite, по 5/6 побед. Повтор: **18,59 против 18,87 мс**. Все тесты прошли.

[Бенчи и отчёт](../benches/results/2026-10-04-parallel-sparse-classifier/report.md). На 1000 строках Lin пока проигрывает; общая цель остаётся открытой.

</details>

<details>
<summary>431. 2026-10-04T18:45:44.505Z</summary>

Теперь займусь малой вставкой. Проверю, можно ли внутри одного batch переиспользовать рассчитанные слоты повторяющихся текстовых токенов, сохранив порядок прибавлений и точные float-биты. Сначала посмотрю текущий код embedding и прежние эксперименты с кэшированием.

</details>

<details>
<summary>432. 2026-10-04T18:47:43.747Z</summary>

Попробую кэш до 32 коротких буквенных ASCII-токенов на один batch. Он будет хранить последовательность слотов, а не суммарный вес: это сохраняет порядок float-операций. Одиночное embedding, числа, длинные и Unicode-токены останутся на прежнем пути.

</details>

<details>
<summary>433. 2026-10-04T18:50:18.051Z</summary>

Семь embedding-тестов прошли, включая границу включения кэша, лимит 32 токена и побайтовое сравнение векторов с прежним embedding. Замеряю native- и durable-вставку: выигрыш на одном пути не компенсирует регрессию на другом.

</details>

<details>
<summary>434. 2026-10-04T18:51:09.194Z</summary>

Первый замер смешанный: native-вставка 10 000 строк ускорилась на 11,70%, но durable-результаты ухудшились при заметном замедлении SQLite-контроля. На 1000 строках устойчивого выигрыша нет. Повторю все четыре кейса с теми же бинарниками, прежде чем решать, сохранять ли кэш.

</details>

<details>
<summary>435. 2026-10-04T18:53:43.093Z</summary>

Повтор не подтвердил выигрыш: native-кейс 10 000 строк стал медленнее на 1,27%, а durable-кейс 1000 строк — на 4,34%. Кэш откатываю. Точные биты он сохраняет, но устойчивого ускорения нужных сценариев не даёт.

</details>

<details>
<summary>436. 2026-10-04T18:54:37.186Z</summary>

Кэш токенов откатил: выигрыш не повторился, а durable-вставка 1000 строк замедлилась на 4,34%.

[Обе серии бенчей](../benches/results/2026-10-04-batch-token-slots/report.md) запушены: `3478392`. Подтверждённые оптимизации WAL сохранены; общая цель ещё не достигнута.

</details>

<details>
<summary>437. 2026-10-04T18:54:42.262Z</summary>

Продолжаю работу над малой вставкой. Посмотрю обновление FTS при batch-insert: здесь можно убрать лишнюю работу, сохранив токены, порядок postings и результаты поиска.

</details>

<details>
<summary>438. 2026-10-04T18:56:28.694Z</summary>

Проверю ASCII-разделение слов в append-пути FTS. Как и в embedding, потребуется исключение для вертикальной табуляции: `split_ascii_whitespace` и `split_whitespace` обрабатывают её по-разному. Unicode-текст останется на прежнем пути; сами postings сравню с эталоном.

</details>

<details>
<summary>439. 2026-10-04T18:58:21.116Z</summary>

FTS-проверки прошли. Эталонное сравнение покрывает все 128 ASCII-символов, Unicode-пробелы, повторные слова и вертикальную табуляцию. Теперь проверяю, даёт ли изменение выигрыш на native- и durable-вставке.

</details>

<details>
<summary>440. 2026-10-04T19:01:31.529Z</summary>

ASCII-разделение FTS не дало выигрыша: native-вставка 10 000 строк замедлилась на 1,68% во всех шести парах, на остальных кейсах результаты смешанные. Откатываю вариант и сохраняю замеры; прежние улучшения WAL остаются.

</details>

<details>
<summary>441. 2026-10-04T19:02:45.467Z</summary>

ASCII-разделение FTS откатил: native-вставка 10 000 строк замедлилась на 1,68% во всех 6 парах.

[Бенчи и отчёт](../benches/results/2026-10-04-fts-ascii-split/report.md) запушены: `588136e`. Подтверждённые улучшения WAL сохранены; общая цель ещё не достигнута.

</details>

<details>
<summary>442. 2026-10-04T19:03:12.038Z</summary>

Последние изменения дали небольшой перевес над SQLite на большой durable-вставке, но общий результат ещё не доказан. Обновлю текущую картину сравнений: проверю результаты малой вставки и стенд для DuckDB, PostgreSQL, MySQL, MongoDB и pandas.

</details>

<details>
<summary>443. 2026-10-04T19:05:31.225Z</summary>

Текущая малая native-вставка всё ещё проигрывает SQLite: 1,160 против 1,115 мс, 0/6 побед. Запускаю обновлённую матрицу чтения на 1k/10k/100k с тремя независимыми процессами. MSSQL и Kusto сейчас не настроены, поэтому их результаты будут отмечены как неполные.

</details>

<details>
<summary>444. 2026-10-04T19:07:26.826Z</summary>

На 1k и 10k матрица завершилась; результаты и строки проверены, тестовые объекты удалены, контрольные данные не изменились. Выполняется 100k. После него отдельно посчитаю победы над доступными движками: общий статус остаётся `incomplete` из-за MSSQL и Kusto.

</details>

<details>
<summary>445. 2026-10-04T19:09:24.643Z</summary>

На 1k и 10k текущий Lin выиграл все 72 доступных сравнения чтения — шесть запросов против шести движков. На 100k идёт третий независимый процесс. В матрицу пока не входят MSSQL/Kusto и результаты вставки: они остаются отдельными незакрытыми проверками.

</details>

<details>
<summary>446. 2026-10-04T19:12:50.389Z</summary>

Обновлённая матрица завершилась: Lin выиграл все 108 доступных сравнений чтения. Наименьший запас — над DuckDB, примерно 1,16× на одном из кейсов; результат относится только к этим API-путям. Контрольные данные сохранены, fixtures удалены, поднятые контейнеры остановлены. MSSQL/Kusto и малая вставка остаются незакрытыми.

</details>

<details>
<summary>447. 2026-10-04T19:14:32.211Z</summary>

Обновил матрицу чтения: **108/108 доступных сравнений в пользу Lin** против SQLite, DuckDB, PostgreSQL, MySQL, MongoDB и pandas.

[Бенчи и отчёт](../benches/results/2026-10-04-current-peer-read-matrix/report.md) запушены: `66d3361`.

Общая цель ещё открыта: MSSQL/Kusto не настроены, малая вставка проигрывает SQLite. Контейнеры остановлены, тестовые данные удалены.

</details>

<details>
<summary>448. 2026-10-04T19:15:02.148Z</summary>

Вернусь к малой durable-вставке. В прежнем эксперименте подсчёт числа nonzero во время записи был быстрее на 1000 строках, но хуже на 10 000. Проверю его только для batches меньше 4096 строк; подтверждённый большой путь сохраню.

</details>

<details>
<summary>449. 2026-10-04T19:18:12.823Z</summary>

Все 11 WAL-тестов прошли, включая backfill после перераспределения буфера и отказ по лимиту декодирования. Идут шесть парных замеров на 1000 и 10 000 строках; большого пути изменение не касается, но его скорость тоже проверю.

</details>

<details>
<summary>450. 2026-10-04T19:18:58.596Z</summary>

Первая серия дала +2,00% на 1000 строках, 4/6 побед; SQLite-контроль почти не изменился. На 10 000 строках устойчивого изменения не видно. Проверю полный набор тестов и повторю те же замеры перед сохранением.

</details>

<details>
<summary>451. 2026-10-04T19:21:55.129Z</summary>

Полный набор тестов прошёл. В повторе проверяю, воспроизводится ли выигрыш на малой вставке; формат WAL, общий лимит декодирования и flush остались прежними.

</details>

<details>
<summary>452. 2026-10-04T19:22:45.124Z</summary>

Повтор не подтвердил выигрыш: 1000 строк замедлились на 4,87%, а 10 000 — на 11,83% при ускорившемся SQLite-контроле. Откатываю backfill: первой положительной серии недостаточно. Все замеры сохраняю.

</details>

<details>
<summary>453. 2026-10-04T19:24:14.880Z</summary>

Backfill для малых batches откатил: первый выигрыш не повторился, вторая серия показала регрессии.

[Замеры и отчёт](../benches/results/2026-10-04-small-sparse-backfill/report.md) запушены: `2f59f1a`. Тесты прошли, прежние оптимизации сохранены. Малая вставка и полный набор конкурентов остаются незакрытыми.

</details>

<details>
<summary>454. 2026-10-04T19:24:53.980Z</summary>

Следующий эксперимент — аллокатор. В профиле малая вставка создаёт много строковых ключей и Arc-значений; проверю, даёт ли другой allocator заметный выигрыш без изменения данных и API. Сначала изучу конфигурацию сборки, затем сравню native- и durable-пути.

</details>

<details>
<summary>455. 2026-10-04T19:27:07.961Z</summary>

Для эксперимента подключу [mimalloc](https://github.com/purpleprotocol/mimalloc_rust) только к benchmark-бинарнику и тестовой сборке. Если выигрыш подтвердится, оформлю выбор allocator для CLI отдельно. Важно: Rust-обёртки SQLite в этом бинарнике тоже сменят allocator, поэтому такой контроль нельзя считать полностью неизменным.

</details>

<details>
<summary>456. 2026-10-04T19:34:16.877Z</summary>

Первый замер с mimalloc дал Lin около 10–11% на вставках в память. На durable-вставках выигрыш слабее, причём SQLite тоже ускорился. Повторю сравнение на тех же бинарниках, прежде чем выбирать allocator для приложения.

</details>

<details>
<summary>457. 2026-10-04T19:36:00.610Z</summary>

Повтор не подтвердил общий выигрыш: с mimalloc вставки 1k в память стали на 9,6% медленнее, durable 1k — на 20,3%. SQLite тоже сильно колебался. Уберу экспериментальный allocator и сохраню сырые замеры: оснований менять его в приложении пока нет.

</details>

<details>
<summary>458. 2026-10-04T19:36:46.310Z</summary>

Проверил mimalloc двумя сериями бенчей. Первоначальное ускорение не подтвердилось: в повторе вставка 1k замедлилась на 9,6%, durable — на 20,3%. Эксперимент откатил полностью.

[Замеры и отчёт](../benches/results/2026-10-04-mimalloc-prototype/README.md) запушены в `main`, коммит `0f811a3`. Нового подтверждённого ускорения в этой серии нет.

</details>

<details>
<summary>459. 2026-10-04T19:41:36.690Z</summary>

Продолжу с узкого места — вставок 1k, где Lin пока уступает SQLite. Проверю текущий путь индексации и выделения памяти, чтобы выбрать следующий эксперимент по коду и замерам.

</details>

<details>
<summary>460. 2026-10-04T19:42:11.181Z</summary>

В нормализации embedding есть ещё один кандидат: сейчас код обходит все 768 координат уже при заполнении 20% вектора. Проверю обход только заполненных координат до 50%; для больших сумм сохраню прежний порядок расчёта. Сначала проверю побитовое совпадение результатов, затем вставки.

</details>

<details>
<summary>461. 2026-10-04T19:43:44.874Z</summary>

Проверки побитового совпадения прошли, включая границу плотности 50% и большие веса, где порядок суммирования важен. Идёт сравнение с текущей реализацией на свежих базах, отдельно в памяти и с Full-sync.

</details>

<details>
<summary>462. 2026-10-04T19:44:53.429Z</summary>

Изменение порога почти не повлияло на вставки 1k: парный выигрыш составил 0,23% в памяти и 0,52% с Full-sync. SQLite всё ещё быстрее. На 10k результаты смешанные, поэтому этот вариант тоже откатил; замеры сохраню в репозитории.

</details>

<details>
<summary>463. 2026-10-04T19:45:13.029Z</summary>

Проверил обход только заполненных координат embedding до плотности 50%. Побитовая совместимость подтверждена, но ускорение вставок 1k — лишь 0,23–0,52%; SQLite остаётся быстрее. Изменение откатил.

[Отчёт и сырые замеры](../benches/results/2026-10-04-normalization-half-density/README.md) запушены: `a7ff491`.

Общая цель ещё не достигнута: остаются вставки 1k и непроверенные MSSQL/Kusto.

</details>

<details>
<summary>464. 2026-10-04T20:12:46.047Z</summary>

Проверю свежие фазы вставки 1k: предыдущие изменения нормализации почти не повлияли на результат, поэтому следующий эксперимент выберу по измеренному времени этапов.

</details>

<details>
<summary>465. 2026-10-04T20:14:02.977Z</summary>

Диагностический бинарник собран; временные таймеры из исходников уже убраны. Замеряю три независимых процесса: сборку строк, проверки, embedding, индексы, упаковку WAL, кодирование, запись и синхронизацию.

</details>

<details>
<summary>466. 2026-10-04T20:15:09.181Z</summary>

Свежий профиль 1k показал: синхронизация — около 1,45 мс, кодирование WAL — 0,93 мс, embedding — 0,54 мс. Для следующего изменения приоритетнее упаковка и кодирование малых пакетов. Условия Full-sync сохраняю.

</details>

<details>
<summary>467. 2026-10-04T20:15:18.566Z</summary>

Обновил профиль вставок на текущем коде: три процесса, свежие базы.

Для 1k: Full-sync ≈1,45 мс, кодирование WAL ≈0,93 мс, embedding ≈0,54 мс. Следующая цель оптимизации — упаковка и кодирование малых пакетов.

[Замеры](../benches/results/2026-10-05-durable-phase-refresh/README.md) запушены в `5001867`. Рабочее дерево чистое. Это диагностика; нового ускорения пока нет, общая цель остаётся открытой.

</details>

<details>
<summary>468. 2026-10-04T20:18:09.871Z</summary>

Проверю следующий вариант для малых WAL-пакетов: запись индекса и значения одной восьмибайтовой парой. Формат и Full-sync сохраню; результат оценю на вставках 1k и 10k.

</details>

<details>
<summary>469. 2026-10-04T20:18:47.679Z</summary>

Восьмибайтовые пары уже проверялись раньше и были отклонены, поэтому повторять их не буду. Нашёл другой кандидат: классификатор WAL уже считает размер sparse-данных, но затем отбрасывает его. Попробую использовать этот размер для резервирования буфера без повторного подсчёта.

</details>

<details>
<summary>470. 2026-10-04T20:20:21.784Z</summary>

Все 10 проверок WAL прошли, включая побитовое сравнение sparse-кодирования и восстановление. Теперь сравниваю вставки на свежих базах: использую уже рассчитанный размер для резерва буфера; кодек и синхронизация прежние.

</details>

<details>
<summary>471. 2026-10-04T20:21:42.400Z</summary>

В первой серии durable 10k ускорился примерно на 20%, выиграв 5 из 6 пар. Для 1k явного преимущества нет: SQLite ускорился сильнее. Повторю те же бинарники, чтобы проверить результат 10k.

</details>

<details>
<summary>472. 2026-10-04T20:23:21.147Z</summary>

Повтор не подтвердил выигрыш 10k: осталось около 3%, быстрее лишь в 3 из 6 пар; durable 1k замедлился на 3,8%. Изменение откатываю. Такой результат не даёт оснований сохранять резервирование по рассчитанному размеру.

</details>

<details>
<summary>473. 2026-10-04T20:23:56.462Z</summary>

Проверил резервирование WAL-буфера по уже рассчитанному sparse-размеру. Первые +20% на durable 10k не повторились; durable 1k замедлился на 3,8%. Изменение полностью откатил.

10 проверок WAL прошли. [Обе серии замеров](../benches/results/2026-10-05-sparse-size-reserve/README.md) запушены в `386300e`. Рабочее дерево чистое; нового подтверждённого ускорения нет, общая цель остаётся открытой.

</details>

<details>
<summary>474. 2026-10-04T20:24:01.537Z</summary>

Соберу историю бенчмарков из репозитория и этого диалога в один Markdown-файл: условия замеров, результаты, принятые и отклонённые оптимизации, а также ссылки на исходные отчёты.

</details>

## Реестр исходных прогонов

Каждый сохранённый run.json перечислен отдельно, включая baseline/candidate, независимые процессы и повторные серии. Полные observations, операции, контракты, provenance и версии следует читать в исходном JSON; здесь не пересчитываются percentiles из batch averages.

| Источник | Статус | Cases | Observations |
|---|---|---:|---:|
| [.airbug-bench/ci/run.json](../.airbug-bench/ci/run.json) | complete | 80 | 640 |
| [.airbug-bench/lin-compare/run.json](../.airbug-bench/lin-compare/run.json) | failed | 0 | 0 |
| [.airbug-bench/smoke/run.json](../.airbug-bench/smoke/run.json) | complete | 1 | 8 |
| [benches/results/0.4.0-core-phases/run.json](../benches/results/0.4.0-core-phases/run.json) | complete | 11 | 1100 |
| [benches/results/0.4.0-fts-phases/run.json](../benches/results/0.4.0-fts-phases/run.json) | complete | 5 | 500 |
| [benches/results/0.4.0-full-70/run.json](../benches/results/0.4.0-full-70/run.json) | complete | 70 | 7000 |
| [benches/results/0.4.0-insert-phases-repeat/run.json](../benches/results/0.4.0-insert-phases-repeat/run.json) | complete | 4 | 400 |
| [benches/results/0.4.0-join-phases-repeat/run.json](../benches/results/0.4.0-join-phases-repeat/run.json) | complete | 2 | 200 |
| [benches/results/2026-10-01-absolute-timestamps/native-bulk-clean/run.json](../benches/results/2026-10-01-absolute-timestamps/native-bulk-clean/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-ascii-lower-ab/native-bulk/run.json](../benches/results/2026-10-01-ascii-lower-ab/native-bulk/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-1-ascii/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-1-ascii/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-1-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-2-ascii/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-2-ascii/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-2-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-3-ascii/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-3-ascii/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-ascii-lower-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-ascii-lower-ab/pair-3-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-1-candidate/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-1-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-2-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-2-candidate/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-2-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-3-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-borrowed-delete-ab/pair-3-candidate/run.json](../benches/results/2026-10-01-borrowed-delete-ab/pair-3-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-bulk-borrowed-tokens/run.json](../benches/results/2026-10-01-bulk-borrowed-tokens/run.json) | complete | 6 | 48 |
| [benches/results/2026-10-01-bulk-exact-norm/run.json](../benches/results/2026-10-01-bulk-exact-norm/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-01-bulk-fts-append/run.json](../benches/results/2026-10-01-bulk-fts-append/run.json) | complete | 8 | 96 |
| [benches/results/2026-10-01-bulk-pack-tracking/run.json](../benches/results/2026-10-01-bulk-pack-tracking/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-1-original/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-1-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-1-sorted/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-1-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-2-original/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-2-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-2-sorted/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-2-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-3-original/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-3-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-ab/pair-3-sorted/run.json](../benches/results/2026-10-01-bulk-tree-ab/pair-3-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-tree-before/run.json](../benches/results/2026-10-01-bulk-tree-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-bulk-wal-count-fixed/run.json](../benches/results/2026-10-01-bulk-wal-count-fixed/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-01-bulk-with-appender/run.json](../benches/results/2026-10-01-bulk-with-appender/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-01-committed-local-read-audit/process-1/run.json](../benches/results/2026-10-01-committed-local-read-audit/process-1/run.json) | complete | 6 | 24 |
| [benches/results/2026-10-01-committed-local-read-audit/process-2/run.json](../benches/results/2026-10-01-committed-local-read-audit/process-2/run.json) | complete | 6 | 24 |
| [benches/results/2026-10-01-committed-local-read-audit/process-3/run.json](../benches/results/2026-10-01-committed-local-read-audit/process-3/run.json) | complete | 6 | 24 |
| [benches/results/2026-10-01-committed-local-read-audit/run.json](../benches/results/2026-10-01-committed-local-read-audit/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-1-original/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-1-original/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-1-owned/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-1-owned/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-2-original/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-2-original/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-2-owned/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-2-owned/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-3-original/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-3-original/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-ab/pair-3-owned/run.json](../benches/results/2026-10-01-delete-owned-result-ab/pair-3-owned/run.json) | complete | 6 | 240 |
| [benches/results/2026-10-01-delete-owned-result-after/run.json](../benches/results/2026-10-01-delete-owned-result-after/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-delete-owned-result-before/run.json](../benches/results/2026-10-01-delete-owned-result-before/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-1-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-1-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-1-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-1-candidate/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-2-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-2-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-2-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-2-candidate/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-3-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-3-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/bulk-3-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/bulk-3-candidate/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-dense-reverse-ab/read-1-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-1-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/read-1-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-1-candidate/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/read-2-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-2-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/read-2-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-2-candidate/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/read-3-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-3-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/read-3-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/read-3-candidate/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-dense-reverse-ab/write-1-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-1-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-dense-reverse-ab/write-1-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-1-candidate/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-dense-reverse-ab/write-2-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-2-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-dense-reverse-ab/write-2-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-2-candidate/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-dense-reverse-ab/write-3-baseline/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-3-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-dense-reverse-ab/write-3-candidate/run.json](../benches/results/2026-10-01-dense-reverse-ab/write-3-candidate/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-1/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-2/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-3/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-4/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-5/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/baseline-6/run.json](../benches/results/2026-10-01-doc-slab-scan/baseline-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-1/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-2/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-3/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-4/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-5/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/candidate-6/run.json](../benches/results/2026-10-01-doc-slab-scan/candidate-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-1/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-2/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-3/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-4/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-5/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-baseline-6/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-1/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-2/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-3/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-4/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-5/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-doc-slab-scan/durable-candidate-6/run.json](../benches/results/2026-10-01-doc-slab-scan/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-duckdb-appender/run.json](../benches/results/2026-10-01-duckdb-appender/run.json) | complete | 2 | 16 |
| [benches/results/2026-10-01-durable-profile/baseline-1/run.json](../benches/results/2026-10-01-durable-profile/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/baseline-2/run.json](../benches/results/2026-10-01-durable-profile/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/baseline-3/run.json](../benches/results/2026-10-01-durable-profile/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/baseline-4/run.json](../benches/results/2026-10-01-durable-profile/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/baseline-5/run.json](../benches/results/2026-10-01-durable-profile/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/baseline-6/run.json](../benches/results/2026-10-01-durable-profile/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-1/run.json](../benches/results/2026-10-01-durable-profile/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-2/run.json](../benches/results/2026-10-01-durable-profile/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-3/run.json](../benches/results/2026-10-01-durable-profile/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-4/run.json](../benches/results/2026-10-01-durable-profile/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-5/run.json](../benches/results/2026-10-01-durable-profile/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-durable-profile/candidate-6/run.json](../benches/results/2026-10-01-durable-profile/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-alternating/process-1-modulo/run.json](../benches/results/2026-10-01-embed-alternating/process-1-modulo/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-alternating/process-1-reciprocal/run.json](../benches/results/2026-10-01-embed-alternating/process-1-reciprocal/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-alternating/process-2-modulo/run.json](../benches/results/2026-10-01-embed-alternating/process-2-modulo/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-alternating/process-2-reciprocal/run.json](../benches/results/2026-10-01-embed-alternating/process-2-reciprocal/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-alternating/process-3-modulo/run.json](../benches/results/2026-10-01-embed-alternating/process-3-modulo/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-alternating/process-3-reciprocal/run.json](../benches/results/2026-10-01-embed-alternating/process-3-reciprocal/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-before/run.json](../benches/results/2026-10-01-embed-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-1/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-2/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-3/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-4/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-5/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/baseline-6/run.json](../benches/results/2026-10-01-embed-default-modulo/baseline-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-1/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-2/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-3/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-4/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-5/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-default-modulo/candidate-6/run.json](../benches/results/2026-10-01-embed-default-modulo/candidate-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-embed-pool-ab/pair-1-pool/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-1-pool/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-pool-ab/pair-1-serial/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-1-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-pool-ab/pair-2-pool/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-2-pool/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-pool-ab/pair-2-serial/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-2-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-pool-ab/pair-3-pool/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-3-pool/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-pool-ab/pair-3-serial/run.json](../benches/results/2026-10-01-embed-pool-ab/pair-3-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-reciprocal/run.json](../benches/results/2026-10-01-embed-reciprocal/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-seen-after/run.json](../benches/results/2026-10-01-embed-seen-after/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-seen-after-repeat/run.json](../benches/results/2026-10-01-embed-seen-after-repeat/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-seen-before/run.json](../benches/results/2026-10-01-embed-seen-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-seen-before-repeat/run.json](../benches/results/2026-10-01-embed-seen-before-repeat/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-1/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-2/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-3/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-4/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-5/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/baseline-6/run.json](../benches/results/2026-10-01-embed-sparse-output/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-1/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-2/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-3/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-4/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-5/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-embed-sparse-output/candidate-6/run.json](../benches/results/2026-10-01-embed-sparse-output/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-1/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-2/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-3/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-4/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-5/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/baseline-6/run.json](../benches/results/2026-10-01-empty-store-uniqueness/baseline-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-1/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-2/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-3/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-4/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-5/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-empty-store-uniqueness/candidate-6/run.json](../benches/results/2026-10-01-empty-store-uniqueness/candidate-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-append-ab/pair-1-append/run.json](../benches/results/2026-10-01-fts-append-ab/pair-1-append/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-ab/pair-1-original/run.json](../benches/results/2026-10-01-fts-append-ab/pair-1-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-ab/pair-2-append/run.json](../benches/results/2026-10-01-fts-append-ab/pair-2-append/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-ab/pair-2-original/run.json](../benches/results/2026-10-01-fts-append-ab/pair-2-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-ab/pair-3-append/run.json](../benches/results/2026-10-01-fts-append-ab/pair-3-append/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-ab/pair-3-original/run.json](../benches/results/2026-10-01-fts-append-ab/pair-3-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-append-before/run.json](../benches/results/2026-10-01-fts-append-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-1/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-2/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-3/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-4/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-5/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/baseline-6/run.json](../benches/results/2026-10-01-fts-entry-ref/baseline-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-1/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-2/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-3/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-4/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-5/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/candidate-6/run.json](../benches/results/2026-10-01-fts-entry-ref/candidate-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-fts-entry-ref/reads-baseline-1/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-baseline-1/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/reads-baseline-2/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-baseline-2/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/reads-baseline-3/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-baseline-3/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/reads-candidate-1/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-candidate-1/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/reads-candidate-2/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-candidate-2/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/reads-candidate-3/run.json](../benches/results/2026-10-01-fts-entry-ref/reads-candidate-3/run.json) | complete | 3 | 48 |
| [benches/results/2026-10-01-fts-entry-ref/writes-baseline-1/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-baseline-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-entry-ref/writes-baseline-2/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-baseline-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-entry-ref/writes-baseline-3/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-baseline-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-entry-ref/writes-candidate-1/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-candidate-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-entry-ref/writes-candidate-2/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-candidate-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-entry-ref/writes-candidate-3/run.json](../benches/results/2026-10-01-fts-entry-ref/writes-candidate-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-fts-fused-correct-ab/native-bulk/run.json](../benches/results/2026-10-01-fts-fused-correct-ab/native-bulk/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-fts-fused-correct-ab/native-delete/run.json](../benches/results/2026-10-01-fts-fused-correct-ab/native-delete/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-1/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-2/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-3/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-4/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-5/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-baseline-6/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-1/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-2/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-3/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-4/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-5/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/lower-candidate-6/run.json](../benches/results/2026-10-01-fts-slab-scratch/lower-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-1/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-2/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-3/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-4/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-5/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-6/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-1/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-2/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-3/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-4/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-5/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-6/run.json](../benches/results/2026-10-01-fts-slab-scratch/mixed-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-fts-sorted-pending/run.json](../benches/results/2026-10-01-fts-sorted-pending/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-index-move-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-index-move-ab/pair-1-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-move-ab/pair-1-move/run.json](../benches/results/2026-10-01-index-move-ab/pair-1-move/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-move-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-index-move-ab/pair-2-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-move-ab/pair-2-move/run.json](../benches/results/2026-10-01-index-move-ab/pair-2-move/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-move-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-index-move-ab/pair-3-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-move-ab/pair-3-move/run.json](../benches/results/2026-10-01-index-move-ab/pair-3-move/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-index-reserve-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-1-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-1-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-1-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-2-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-2-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-2-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-3-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-3-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-3-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-4-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-4-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-4-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-4-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-5-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-5-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-5-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-5-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-6-baseline/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-6-baseline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-reserve-ab/pair-6-reserve/run.json](../benches/results/2026-10-01-index-reserve-ab/pair-6-reserve/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-index-same-key/baseline-1/run.json](../benches/results/2026-10-01-index-same-key/baseline-1/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-same-key/baseline-2/run.json](../benches/results/2026-10-01-index-same-key/baseline-2/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-same-key/baseline-3/run.json](../benches/results/2026-10-01-index-same-key/baseline-3/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-same-key/candidate-1/run.json](../benches/results/2026-10-01-index-same-key/candidate-1/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-same-key/candidate-2/run.json](../benches/results/2026-10-01-index-same-key/candidate-2/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-same-key/candidate-3/run.json](../benches/results/2026-10-01-index-same-key/candidate-3/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-index-visit-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-1-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-1-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-1-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-2-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-2-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-2-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-3-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-3-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-3-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-4-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-4-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-4-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-4-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-5-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-5-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-5-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-5-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-6-baseline/run.json](../benches/results/2026-10-01-index-visit-ab/pair-6-baseline/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-index-visit-ab/pair-6-candidate/run.json](../benches/results/2026-10-01-index-visit-ab/pair-6-candidate/run.json) | complete | 12 | 192 |
| [benches/results/2026-10-01-inline-index-ab/final-native-bulk/run.json](../benches/results/2026-10-01-inline-index-ab/final-native-bulk/run.json) | complete | 8 | 96 |
| [benches/results/2026-10-01-inline-index-ab/final-read-1-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-1-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-1-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-1-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-2-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-2-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-2-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-2-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-3-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-3-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-3-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-3-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-4-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-4-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-4-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-4-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-5-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-5-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-5-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-5-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-6-final/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-6-final/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/final-read-6-vec/run.json](../benches/results/2026-10-01-inline-index-ab/final-read-6-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-1-inline/run.json](../benches/results/2026-10-01-inline-index-ab/pair-1-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-1-vec/run.json](../benches/results/2026-10-01-inline-index-ab/pair-1-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-2-inline/run.json](../benches/results/2026-10-01-inline-index-ab/pair-2-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-2-vec/run.json](../benches/results/2026-10-01-inline-index-ab/pair-2-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-3-inline/run.json](../benches/results/2026-10-01-inline-index-ab/pair-3-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/pair-3-vec/run.json](../benches/results/2026-10-01-inline-index-ab/pair-3-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-1-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-1-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-1-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-1-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-2-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-2-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-2-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-2-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-3-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-3-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-3-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-3-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-4-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-4-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-4-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-4-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-5-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-5-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-5-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-5-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-6-inline/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-6-inline/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/rapid-6-vec/run.json](../benches/results/2026-10-01-inline-index-ab/rapid-6-vec/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-1-inline/run.json](../benches/results/2026-10-01-inline-index-ab/read-1-inline/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-1-vec/run.json](../benches/results/2026-10-01-inline-index-ab/read-1-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-2-inline/run.json](../benches/results/2026-10-01-inline-index-ab/read-2-inline/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-2-vec/run.json](../benches/results/2026-10-01-inline-index-ab/read-2-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-3-inline/run.json](../benches/results/2026-10-01-inline-index-ab/read-3-inline/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-inline-index-ab/read-3-vec/run.json](../benches/results/2026-10-01-inline-index-ab/read-3-vec/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-01-insert-borrowed-text/run.json](../benches/results/2026-10-01-insert-borrowed-text/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-insert-phases/run.json](../benches/results/2026-10-01-insert-phases/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-insert-profile/run.json](../benches/results/2026-10-01-insert-profile/run.json) | complete | 1 | 400 |
| [benches/results/2026-10-01-insert-templates-ab/pair-1-original/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-1-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-insert-templates-ab/pair-1-templates/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-1-templates/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-insert-templates-ab/pair-2-original/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-2-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-insert-templates-ab/pair-2-templates/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-2-templates/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-insert-templates-ab/pair-3-original/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-3-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-insert-templates-ab/pair-3-templates/run.json](../benches/results/2026-10-01-insert-templates-ab/pair-3-templates/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-map-position-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-map-position-ab/pair-1-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-map-position-ab/pair-1-move/run.json](../benches/results/2026-10-01-map-position-ab/pair-1-move/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-map-position-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-map-position-ab/pair-2-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-map-position-ab/pair-2-move/run.json](../benches/results/2026-10-01-map-position-ab/pair-2-move/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-map-position-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-map-position-ab/pair-3-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-map-position-ab/pair-3-move/run.json](../benches/results/2026-10-01-map-position-ab/pair-3-move/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-1/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-1/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-2/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-2/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-3/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-3/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-4/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-4/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-5/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-5/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-baseline-6/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-baseline-6/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-1/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-1/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-2/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-2/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-3/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-3/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-4/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-4/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-5/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-5/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-merge-proposal/hybrid-candidate-6/run.json](../benches/results/2026-10-01-merge-proposal/hybrid-candidate-6/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-native-after-wal/run-1/run.json](../benches/results/2026-10-01-native-after-wal/run-1/run.json) | complete | 10 | 240 |
| [benches/results/2026-10-01-native-after-wal/run-2/run.json](../benches/results/2026-10-01-native-after-wal/run-2/run.json) | complete | 10 | 240 |
| [benches/results/2026-10-01-native-after-wal/run-3/run.json](../benches/results/2026-10-01-native-after-wal/run-3/run.json) | complete | 10 | 240 |
| [benches/results/2026-10-01-native-core/run.json](../benches/results/2026-10-01-native-core/run.json) | complete | 68 | 544 |
| [benches/results/2026-10-01-norm-alternating/process-1-exact_sum/run.json](../benches/results/2026-10-01-norm-alternating/process-1-exact_sum/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-norm-alternating/process-1-sorted/run.json](../benches/results/2026-10-01-norm-alternating/process-1-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-norm-alternating/process-2-exact_sum/run.json](../benches/results/2026-10-01-norm-alternating/process-2-exact_sum/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-norm-alternating/process-2-sorted/run.json](../benches/results/2026-10-01-norm-alternating/process-2-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-norm-alternating/process-3-exact_sum/run.json](../benches/results/2026-10-01-norm-alternating/process-3-exact_sum/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-norm-alternating/process-3-sorted/run.json](../benches/results/2026-10-01-norm-alternating/process-3-sorted/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-1-cached/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-1-cached/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-1-original/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-1-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-2-cached/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-2-cached/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-2-original/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-2-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-3-cached/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-3-cached/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-ab/pair-3-original/run.json](../benches/results/2026-10-01-pair-slots-ab/pair-3-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-pair-slots-before/run.json](../benches/results/2026-10-01-pair-slots-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-1-parallel/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-1-parallel/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-1-serial/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-1-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-2-parallel/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-2-parallel/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-2-serial/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-2-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-3-parallel/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-3-parallel/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-ab/pair-3-serial/run.json](../benches/results/2026-10-01-parallel-embed-ab/pair-3-serial/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-parallel-embed-before/run.json](../benches/results/2026-10-01-parallel-embed-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-peer-100k/process-1/run.json](../benches/results/2026-10-01-peer-100k/process-1/run.json) | complete | 6 | 42 |
| [benches/results/2026-10-01-peer-100k/process-2/run.json](../benches/results/2026-10-01-peer-100k/process-2/run.json) | complete | 6 | 42 |
| [benches/results/2026-10-01-peer-100k/process-3/run.json](../benches/results/2026-10-01-peer-100k/process-3/run.json) | complete | 6 | 42 |
| [benches/results/2026-10-01-peer-100k/run.json](../benches/results/2026-10-01-peer-100k/run.json) | complete | 6 | 126 |
| [benches/results/2026-10-01-peer-10k/run.json](../benches/results/2026-10-01-peer-10k/run.json) | complete | 6 | 126 |
| [benches/results/2026-10-01-peer-current-100k/process-1/run.json](../benches/results/2026-10-01-peer-current-100k/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-01-peer-current-100k/process-2/run.json](../benches/results/2026-10-01-peer-current-100k/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-01-peer-current-100k/process-3/run.json](../benches/results/2026-10-01-peer-current-100k/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-01-peer-current-100k/run.json](../benches/results/2026-10-01-peer-current-100k/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-01-pg-owned-schema/validation/run.json](../benches/results/2026-10-01-pg-owned-schema/validation/run.json) | complete | 11 | 11 |
| [benches/results/2026-10-01-pg-validated-insert/run-1/run.json](../benches/results/2026-10-01-pg-validated-insert/run-1/run.json) | complete | 10 | 80 |
| [benches/results/2026-10-01-pg-validated-insert/run-2/run.json](../benches/results/2026-10-01-pg-validated-insert/run-2/run.json) | complete | 10 | 80 |
| [benches/results/2026-10-01-pg-validated-insert/run-3/run.json](../benches/results/2026-10-01-pg-validated-insert/run-3/run.json) | complete | 10 | 80 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-1-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-1-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-1-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-1-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-2-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-2-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-2-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-2-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-3-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-3-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-3-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-3-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/fixed-read/run.json](../benches/results/2026-10-01-posting-tail-ab/fixed-read/run.json) | complete | 1 | 12 |
| [benches/results/2026-10-01-posting-tail-ab/live-fixed-check/run.json](../benches/results/2026-10-01-posting-tail-ab/live-fixed-check/run.json) | complete | 2 | 24 |
| [benches/results/2026-10-01-posting-tail-ab/only-1-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/only-1-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/only-1-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/only-1-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/only-2-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/only-2-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/only-2-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/only-2-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/only-3-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/only-3-baseline/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/only-3-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/only-3-candidate/run.json) | complete | 12 | 288 |
| [benches/results/2026-10-01-posting-tail-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-1-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-posting-tail-ab/pair-1-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-1-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-posting-tail-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-2-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-posting-tail-ab/pair-2-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-2-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-posting-tail-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-3-baseline/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-posting-tail-ab/pair-3-candidate/run.json](../benches/results/2026-10-01-posting-tail-ab/pair-3-candidate/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-1-cells/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-1-cells/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-1-original/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-1-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-2-cells/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-2-cells/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-2-original/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-2-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-3-cells/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-3-cells/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-cells-ab/pair-3-original/run.json](../benches/results/2026-10-01-prepared-cells-ab/pair-3-original/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-1/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-2/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-3/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-4/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-5/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/baseline-6/run.json](../benches/results/2026-10-01-prepared-sorted-records/baseline-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-1/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-1/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-2/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-2/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-3/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-3/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-4/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-4/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-5/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-5/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-prepared-sorted-records/candidate-6/run.json](../benches/results/2026-10-01-prepared-sorted-records/candidate-6/run.json) | complete | 6 | 144 |
| [benches/results/2026-10-01-row-build-after/run.json](../benches/results/2026-10-01-row-build-after/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-row-build-before/run.json](../benches/results/2026-10-01-row-build-before/run.json) | complete | 4 | 48 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-1/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-2/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-3/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-4/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-5/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-baseline-6/run.json](../benches/results/2026-10-01-segmented-embedding/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-1/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-2/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-3/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-4/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-5/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/durable-candidate-6/run.json](../benches/results/2026-10-01-segmented-embedding/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-1/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-2/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-3/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-4/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-5/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-baseline-6/run.json](../benches/results/2026-10-01-segmented-embedding/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-1/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-2/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-3/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-4/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-5/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-segmented-embedding/native-candidate-6/run.json](../benches/results/2026-10-01-segmented-embedding/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-serial-final-bulk/run.json](../benches/results/2026-10-01-serial-final-bulk/run.json) | complete | 8 | 96 |
| [benches/results/2026-10-01-shared-identity-ab/native-bulk/run.json](../benches/results/2026-10-01-shared-identity-ab/native-bulk/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-shared-identity-ab/native-delete/run.json](../benches/results/2026-10-01-shared-identity-ab/native-delete/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-shared-identity-ab/native-delete-clean/run.json](../benches/results/2026-10-01-shared-identity-ab/native-delete-clean/run.json) | complete | 6 | 72 |
| [benches/results/2026-10-01-shared-identity-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-1-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-1-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-1-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-2-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-2-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-2-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-3-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-3-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-3-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-4-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-4-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-4-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-4-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-5-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-5-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-5-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-5-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-6-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-6-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/pair-6-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/pair-6-shared/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/read-1-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-1-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-1-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-1-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-2-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-2-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-2-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-2-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-3-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-3-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-3-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-3-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-4-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-4-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-4-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-4-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-5-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-5-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-5-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-5-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-6-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/read-6-baseline/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/read-6-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/read-6-shared/run.json) | complete | 1 | 24 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-1-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-1-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-1-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-1-uri-only/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-2-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-2-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-2-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-2-uri-only/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-3-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-3-baseline/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-pair-3-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-pair-3-uri-only/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-1-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-1-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-1-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-1-uri-only/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-2-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-2-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-2-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-2-uri-only/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-3-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-3-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/uri-write-3-uri-only/run.json](../benches/results/2026-10-01-shared-identity-ab/uri-write-3-uri-only/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-1-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/write-1-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-1-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/write-1-shared/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-2-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/write-2-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-2-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/write-2-shared/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-3-baseline/run.json](../benches/results/2026-10-01-shared-identity-ab/write-3-baseline/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-identity-ab/write-3-shared/run.json](../benches/results/2026-10-01-shared-identity-ab/write-3-shared/run.json) | complete | 12 | 144 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-1/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-2/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-3/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-4/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-5/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-shared-wal-vectors/baseline-6/run.json](../benches/results/2026-10-01-shared-wal-vectors/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-1/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-2/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-3/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-4/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-5/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-shared-wal-vectors/candidate-6/run.json](../benches/results/2026-10-01-shared-wal-vectors/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-sparse-single-pass/baseline-1/run.json](../benches/results/2026-10-01-sparse-single-pass/baseline-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-single-pass/baseline-2/run.json](../benches/results/2026-10-01-sparse-single-pass/baseline-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-single-pass/baseline-3/run.json](../benches/results/2026-10-01-sparse-single-pass/baseline-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-single-pass/candidate-1/run.json](../benches/results/2026-10-01-sparse-single-pass/candidate-1/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-single-pass/candidate-2/run.json](../benches/results/2026-10-01-sparse-single-pass/candidate-2/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-single-pass/candidate-3/run.json](../benches/results/2026-10-01-sparse-single-pass/candidate-3/run.json) | complete | 4 | 64 |
| [benches/results/2026-10-01-sparse-vector-wal/durable-insert/run.json](../benches/results/2026-10-01-sparse-vector-wal/durable-insert/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-sparse-vector-wal/durable-validated-1/run.json](../benches/results/2026-10-01-sparse-vector-wal/durable-validated-1/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-sparse-vector-wal/durable-validated-2/run.json](../benches/results/2026-10-01-sparse-vector-wal/durable-validated-2/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-sparse-vector-wal/durable-validated-3/run.json](../benches/results/2026-10-01-sparse-vector-wal/durable-validated-3/run.json) | complete | 4 | 32 |
| [benches/results/2026-10-01-stream-delete-ab/pair-1-baseline/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-1-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-stream-delete-ab/pair-1-candidate/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-1-candidate/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-stream-delete-ab/pair-2-baseline/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-2-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-stream-delete-ab/pair-2-candidate/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-2-candidate/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-stream-delete-ab/pair-3-baseline/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-3-baseline/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-stream-delete-ab/pair-3-candidate/run.json](../benches/results/2026-10-01-stream-delete-ab/pair-3-candidate/run.json) | complete | 6 | 96 |
| [benches/results/2026-10-01-substr-packed/process-1/run.json](../benches/results/2026-10-01-substr-packed/process-1/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-packed/process-2/run.json](../benches/results/2026-10-01-substr-packed/process-2/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-packed/process-3/run.json](../benches/results/2026-10-01-substr-packed/process-3/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-packed/run.json](../benches/results/2026-10-01-substr-packed/run.json) | complete | 1 | 6 |
| [benches/results/2026-10-01-substr-short-scan/process-1/run.json](../benches/results/2026-10-01-substr-short-scan/process-1/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-short-scan/process-2/run.json](../benches/results/2026-10-01-substr-short-scan/process-2/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-short-scan/process-3/run.json](../benches/results/2026-10-01-substr-short-scan/process-3/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-01-substr-short-scan/run.json](../benches/results/2026-10-01-substr-short-scan/run.json) | complete | 1 | 6 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-1/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-2/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-3/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-4/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-5/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/baseline-6/run.json](../benches/results/2026-10-01-wal-cell-refs/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-1/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-2/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-3/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-4/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-5/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-cell-refs/candidate-6/run.json](../benches/results/2026-10-01-wal-cell-refs/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-1/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-2/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-3/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-4/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-5/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/baseline-6/run.json](../benches/results/2026-10-01-wal-shared-text/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-1/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-2/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-3/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-4/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-5/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-shared-text/candidate-6/run.json](../benches/results/2026-10-01-wal-shared-text/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-1/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-2/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-3/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-4/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-5/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/baseline-6/run.json](../benches/results/2026-10-01-wal-uniform-fields/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-1/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-2/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-3/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-4/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-5/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-uniform-fields/candidate-6/run.json](../benches/results/2026-10-01-wal-uniform-fields/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-1/run.json](../benches/results/2026-10-01-wal-zstd/baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-2/run.json](../benches/results/2026-10-01-wal-zstd/baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-3/run.json](../benches/results/2026-10-01-wal-zstd/baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-4/run.json](../benches/results/2026-10-01-wal-zstd/baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-5/run.json](../benches/results/2026-10-01-wal-zstd/baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/baseline-6/run.json](../benches/results/2026-10-01-wal-zstd/baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-1/run.json](../benches/results/2026-10-01-wal-zstd/candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-2/run.json](../benches/results/2026-10-01-wal-zstd/candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-3/run.json](../benches/results/2026-10-01-wal-zstd/candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-4/run.json](../benches/results/2026-10-01-wal-zstd/candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-5/run.json](../benches/results/2026-10-01-wal-zstd/candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-wal-zstd/candidate-6/run.json](../benches/results/2026-10-01-wal-zstd/candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-01-write-after/run.json](../benches/results/2026-10-01-write-after/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-write-before-verified/run.json](../benches/results/2026-10-01-write-before-verified/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-write-borrowed-tokens/run.json](../benches/results/2026-10-01-write-borrowed-tokens/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-write-final/run.json](../benches/results/2026-10-01-write-final/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-01-write-owned-move/run.json](../benches/results/2026-10-01-write-owned-move/run.json) | complete | 12 | 96 |
| [benches/results/2026-10-02-mysql-owned-native/final/run-1/run.json](../benches/results/2026-10-02-mysql-owned-native/final/run-1/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/final/run-2/run.json](../benches/results/2026-10-02-mysql-owned-native/final/run-2/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/final/run-3/run.json](../benches/results/2026-10-02-mysql-owned-native/final/run-3/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/final/safety/run.json](../benches/results/2026-10-02-mysql-owned-native/final/safety/run.json) | complete | 11 | 11 |
| [benches/results/2026-10-02-mysql-owned-native/run-1/run.json](../benches/results/2026-10-02-mysql-owned-native/run-1/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/run-2/run.json](../benches/results/2026-10-02-mysql-owned-native/run-2/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/run-3/run.json](../benches/results/2026-10-02-mysql-owned-native/run-3/run.json) | complete | 8 | 64 |
| [benches/results/2026-10-02-mysql-owned-native/safety/run.json](../benches/results/2026-10-02-mysql-owned-native/safety/run.json) | complete | 11 | 11 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-1/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-2/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-3/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-4/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-5/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-baseline-6/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-1/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-2/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-3/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-4/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-5/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/durable-candidate-6/run.json](../benches/results/2026-10-03-embed-bigram-table/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-1/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-2/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-3/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-4/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-5/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-baseline-6/run.json](../benches/results/2026-10-03-embed-bigram-table/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-1/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-2/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-3/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-4/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-5/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/native-candidate-6/run.json](../benches/results/2026-10-03-embed-bigram-table/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-1/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-2/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-3/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-4/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-4/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-5/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-5/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-baseline-6/run.json](../benches/results/2026-10-03-embed-bigram-table/read-baseline-6/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-1/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-2/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-3/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-4/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-4/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-5/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-5/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-bigram-table/read-candidate-6/run.json](../benches/results/2026-10-03-embed-bigram-table/read-candidate-6/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-1/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-2/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-3/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-4/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-5/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-baseline-6/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-1/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-2/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-3/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-4/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-5/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/durable-candidate-6/run.json](../benches/results/2026-10-03-embed-zero-marker/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-1/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-2/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-3/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-4/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-5/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-baseline-6/run.json](../benches/results/2026-10-03-embed-zero-marker/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-1/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-2/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-3/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-4/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-5/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-embed-zero-marker/native-candidate-6/run.json](../benches/results/2026-10-03-embed-zero-marker/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-1/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-2/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-3/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-4/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-5/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-baseline-6/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-1/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-2/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-3/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-4/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-5/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/durable-candidate-6/run.json](../benches/results/2026-10-03-fts-inline-postings/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-1/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-2/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-3/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-4/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-5/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-baseline-6/run.json](../benches/results/2026-10-03-fts-inline-postings/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-1/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-2/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-3/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-4/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-5/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/native-candidate-6/run.json](../benches/results/2026-10-03-fts-inline-postings/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-1/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-2/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-3/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-4/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-5/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-baseline-6/run.json](../benches/results/2026-10-03-fts-inline-postings/read-baseline-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-1/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-2/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-3/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-4/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-5/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-postings/read-candidate-6/run.json](../benches/results/2026-10-03-fts-inline-postings/read-candidate-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-1/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-2/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-3/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-4/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-5/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/baseline-6/run.json](../benches/results/2026-10-03-fts-inline-tokens/baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-1/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-2/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-3/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-4/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-5/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-inline-tokens/candidate-6/run.json](../benches/results/2026-10-03-fts-inline-tokens/candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-1/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-2/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-3/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-4/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-5/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/baseline-6/run.json](../benches/results/2026-10-03-fts-sorted-union/baseline-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-1/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-2/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-3/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-4/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-5/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-fts-sorted-union/candidate-6/run.json](../benches/results/2026-10-03-fts-sorted-union/candidate-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-1/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-2/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-3/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-4/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-5/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-baseline-6/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-1/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-2/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-3/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-4/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-5/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/durable-candidate-6/run.json](../benches/results/2026-10-03-insert-borrowed-text/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-1/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-2/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-3/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-4/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-5/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-baseline-6/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-1/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-2/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-3/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-4/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-5/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-borrowed-text/native-candidate-6/run.json](../benches/results/2026-10-03-insert-borrowed-text/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-insert-phase-profile/process-1/run.json](../benches/results/2026-10-03-insert-phase-profile/process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-insert-phase-profile/process-2/run.json](../benches/results/2026-10-03-insert-phase-profile/process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-insert-phase-profile/process-3/run.json](../benches/results/2026-10-03-insert-phase-profile/process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-1/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-2/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-3/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-4/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-5/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/baseline-6/run.json](../benches/results/2026-10-03-lex-borrow-lower/baseline-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-1/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-1/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-2/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-2/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-3/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-3/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-4/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-4/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-5/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-5/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-lex-borrow-lower/candidate-6/run.json](../benches/results/2026-10-03-lex-borrow-lower/candidate-6/run.json) | complete | 3 | 96 |
| [benches/results/2026-10-03-mongo-native/rows-1000/process-1/run.json](../benches/results/2026-10-03-mongo-native/rows-1000/process-1/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-1000/process-2/run.json](../benches/results/2026-10-03-mongo-native/rows-1000/process-2/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-1000/process-3/run.json](../benches/results/2026-10-03-mongo-native/rows-1000/process-3/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-1000/run.json](../benches/results/2026-10-03-mongo-native/rows-1000/run.json) | complete | 1 | 6 |
| [benches/results/2026-10-03-mongo-native/rows-10000/process-1/run.json](../benches/results/2026-10-03-mongo-native/rows-10000/process-1/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-10000/process-2/run.json](../benches/results/2026-10-03-mongo-native/rows-10000/process-2/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-10000/process-3/run.json](../benches/results/2026-10-03-mongo-native/rows-10000/process-3/run.json) | complete | 1 | 2 |
| [benches/results/2026-10-03-mongo-native/rows-10000/run.json](../benches/results/2026-10-03-mongo-native/rows-10000/run.json) | complete | 1 | 6 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-1/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-2/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-3/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-4/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-5/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-baseline-6/run.json](../benches/results/2026-10-03-parallel-row-build/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-1/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-2/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-3/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-4/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-5/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/durable-candidate-6/run.json](../benches/results/2026-10-03-parallel-row-build/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-1/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-2/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-3/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-4/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-5/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-baseline-6/run.json](../benches/results/2026-10-03-parallel-row-build/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-1/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-2/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-3/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-4/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-5/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-parallel-row-build/native-candidate-6/run.json](../benches/results/2026-10-03-parallel-row-build/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-peer-read-refresh/rows-1000/process-1/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-1000/process-2/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-1000/process-3/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-1000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-1000/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-1000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-03-peer-read-refresh/rows-10000/process-1/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-10000/process-2/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-10000/process-3/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-10000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-10000/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-10000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-03-peer-read-refresh/rows-100000/process-1/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-100000/process-2/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-100000/process-3/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-100000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-03-peer-read-refresh/rows-100000/run.json](../benches/results/2026-10-03-peer-read-refresh/rows-100000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-4/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-5/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-baseline-6/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-4/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-5/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/durable-candidate-6/run.json](../benches/results/2026-10-03-shared-ast-strings/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-4/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-5/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-baseline-6/run.json](../benches/results/2026-10-03-shared-ast-strings/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-4/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-5/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/native-candidate-6/run.json](../benches/results/2026-10-03-shared-ast-strings/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_eq-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-filter_range-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-4/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-4/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-5/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-5/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-6/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-baseline-6/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-4/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-4/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-5/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-5/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-6/run.json](../benches/results/2026-10-03-shared-ast-strings/read-fts_lex_common-candidate-6/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-materialize-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-point_get-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-baseline-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-1/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-1/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-2/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-2/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-3/run.json](../benches/results/2026-10-03-shared-ast-strings/read-text_substr-candidate-3/run.json) | complete | 1 | 32 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-1/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-2/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-3/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-4/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-5/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-baseline-6/run.json](../benches/results/2026-10-03-wal-block-mask/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-1/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-2/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-3/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-4/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-5/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-block-mask/durable-candidate-6/run.json](../benches/results/2026-10-03-wal-block-mask/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-1/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-2/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-3/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-4/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-5/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-baseline-6/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-1/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-2/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-3/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-4/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-5/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-cached-counts/durable-candidate-6/run.json](../benches/results/2026-10-03-wal-cached-counts/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-profile/process-1/run.json](../benches/results/2026-10-03-wal-profile/process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-wal-profile/process-2/run.json](../benches/results/2026-10-03-wal-profile/process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-wal-profile/process-3/run.json](../benches/results/2026-10-03-wal-profile/process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-1/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-2/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-3/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-4/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-5/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-baseline-6/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-1/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-2/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-3/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-4/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-5/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-03-wal-zero-blocks/durable-candidate-6/run.json](../benches/results/2026-10-03-wal-zero-blocks/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-1/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-2/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-3/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-4/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-5/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-baseline-6/run.json](../benches/results/2026-10-04-batch-token-slots/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-1/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-2/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-3/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-4/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-5/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/durable-candidate-6/run.json](../benches/results/2026-10-04-batch-token-slots/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-1/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-2/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-3/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-4/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-5/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-baseline-6/run.json](../benches/results/2026-10-04-batch-token-slots/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-1/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-2/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-3/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-4/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-5/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/native-candidate-6/run.json](../benches/results/2026-10-04-batch-token-slots/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-1/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-2/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-3/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-4/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-5/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-6/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-1/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-2/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-3/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-4/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-5/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-6/run.json](../benches/results/2026-10-04-batch-token-slots/repeat-native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-current-insert-profile/process-1/run.json](../benches/results/2026-10-04-current-insert-profile/process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-current-insert-profile/process-2/run.json](../benches/results/2026-10-04-current-insert-profile/process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-current-insert-profile/process-3/run.json](../benches/results/2026-10-04-current-insert-profile/process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-1/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-2/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-3/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-1000/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-1000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-1/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-2/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-3/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-10000/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-10000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-1/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-1/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-2/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-2/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-3/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/process-3/run.json) | incomplete | 6 | 42 |
| [benches/results/2026-10-04-current-peer-read-matrix/rows-100000/run.json](../benches/results/2026-10-04-current-peer-read-matrix/rows-100000/run.json) | incomplete | 6 | 126 |
| [benches/results/2026-10-04-durable-phase-refresh/column-process-1/run.json](../benches/results/2026-10-04-durable-phase-refresh/column-process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-durable-phase-refresh/column-process-2/run.json](../benches/results/2026-10-04-durable-phase-refresh/column-process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-durable-phase-refresh/column-process-3/run.json](../benches/results/2026-10-04-durable-phase-refresh/column-process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-durable-phase-refresh/process-1/run.json](../benches/results/2026-10-04-durable-phase-refresh/process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-durable-phase-refresh/process-2/run.json](../benches/results/2026-10-04-durable-phase-refresh/process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-durable-phase-refresh/process-3/run.json](../benches/results/2026-10-04-durable-phase-refresh/process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-1/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-2/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-3/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-4/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-5/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-baseline-6/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-1/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-2/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-3/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-4/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-5/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/durable-candidate-6/run.json](../benches/results/2026-10-04-embed-ascii-split/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-1/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-2/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-3/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-4/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-5/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-baseline-6/run.json](../benches/results/2026-10-04-embed-ascii-split/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-1/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-2/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-3/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-4/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-5/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/native-candidate-6/run.json](../benches/results/2026-10-04-embed-ascii-split/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-1/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-2/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-3/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-4/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-5/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-6/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-1/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-2/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-3/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-4/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-5/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-6/run.json](../benches/results/2026-10-04-embed-ascii-split/repeat-native-candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-1/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-2/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-3/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-4/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-5/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-6/run.json](../benches/results/2026-10-04-embed-fixed-trigrams/repeat-native-candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-1/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-2/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-3/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-4/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-5/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-baseline-6/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-1/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-2/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-3/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-4/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-5/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/durable-candidate-6/run.json](../benches/results/2026-10-04-embed-trigram-table/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-1/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-2/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-3/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-4/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-5/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-baseline-6/run.json](../benches/results/2026-10-04-embed-trigram-table/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-1/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-2/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-3/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-4/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-5/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-embed-trigram-table/native-candidate-6/run.json](../benches/results/2026-10-04-embed-trigram-table/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-1/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-2/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-3/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-4/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-5/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-6/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-1/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-2/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-3/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-4/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-5/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-6/run.json](../benches/results/2026-10-04-four-way-sparse-wal/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-four-way-sparse-wal/repeat-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-1/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-2/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-3/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-4/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-5/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-baseline-6/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-1/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-2/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-3/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-4/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-5/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/durable-candidate-6/run.json](../benches/results/2026-10-04-fts-ascii-split/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-1/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-2/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-3/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-4/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-5/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-baseline-6/run.json](../benches/results/2026-10-04-fts-ascii-split/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-1/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-2/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-3/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-4/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-5/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-fts-ascii-split/native-candidate-6/run.json](../benches/results/2026-10-04-fts-ascii-split/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-baseline-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/durable-candidate-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-baseline-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/native-candidate-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-1/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-2/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-3/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-4/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-5/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-6/run.json](../benches/results/2026-10-04-mimalloc-prototype/repeat/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-1/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-2/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-3/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-4/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-5/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-baseline-6/run.json](../benches/results/2026-10-04-normalization-half-density/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-1/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-2/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-3/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-4/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-5/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/durable-candidate-6/run.json](../benches/results/2026-10-04-normalization-half-density/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-1/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-2/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-3/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-4/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-5/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-baseline-6/run.json](../benches/results/2026-10-04-normalization-half-density/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-1/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-2/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-3/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-4/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-5/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-normalization-half-density/native-candidate-6/run.json](../benches/results/2026-10-04-normalization-half-density/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-1/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-2/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-3/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-4/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-5/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-6/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-1/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-2/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-3/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-4/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-5/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-6/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-parallel-sparse-classifier/repeat-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/final-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-1/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-2/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-3/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-4/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-5/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-6/run.json](../benches/results/2026-10-04-parallel-sparse-wal/repeat-final-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-baseline-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/durable-candidate-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/final-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-row-major-wal-pack/repeat-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-1/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-2/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-3/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-4/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-5/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-baseline-6/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-1/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-2/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-3/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-4/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-5/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/durable-candidate-6/run.json](../benches/results/2026-10-04-row-worker-fallback/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-1/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-2/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-3/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-4/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-5/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-baseline-6/run.json](../benches/results/2026-10-04-row-worker-fallback/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-1/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-2/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-3/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-4/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-5/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-fallback/native-candidate-6/run.json](../benches/results/2026-10-04-row-worker-fallback/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-1/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-2/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-3/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-4/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-5/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-6/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-1/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-2/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-3/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-4/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-5/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-6/run.json](../benches/results/2026-10-04-row-worker-low-threshold/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-1/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-2/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-3/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-4/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-5/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-baseline-6/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-1/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-2/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-3/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-4/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-5/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-row-worker-low-threshold/native-candidate-6/run.json](../benches/results/2026-10-04-row-worker-low-threshold/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-baseline-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/durable-candidate-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-baseline-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/native-candidate-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-1/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-2/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-3/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-4/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-5/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-6/run.json](../benches/results/2026-10-04-scoped-batch-embed/outlined-native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-1/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-2/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-3/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-4/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-5/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-baseline-6/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-1/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-2/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-3/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-4/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-5/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/durable-candidate-6/run.json](../benches/results/2026-10-04-small-sparse-backfill/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-small-sparse-backfill/repeat-durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-1/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-2/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-3/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-4/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-5/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-6/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-1/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-2/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-3/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-4/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-5/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-6/run.json](../benches/results/2026-10-04-vector-dictionary-buffered/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-1/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-2/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-3/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-4/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-5/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-6/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-1/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-2/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-3/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-4/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-5/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-6/run.json](../benches/results/2026-10-04-vector-dictionary-codec/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-1/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-2/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-3/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-4/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-5/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-6/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-1/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-2/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-3/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-4/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-5/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-6/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-wal-narrow-coordinates/repeat-durable-candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-1/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-2/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-3/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-4/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-5/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-baseline-6/run.json](../benches/results/2026-10-04-wal-paired-write/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-1/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-2/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-3/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-4/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-5/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/durable-candidate-6/run.json](../benches/results/2026-10-04-wal-paired-write/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-1/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-2/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-3/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-4/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-5/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-6/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-1/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-2/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-3/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-4/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-5/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-6/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-durable-candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-1/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-2/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-3/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-4/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-5/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-6/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-baseline-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-1/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-2/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-3/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-4/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-4/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-5/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-5/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-6/run.json](../benches/results/2026-10-04-wal-paired-write/repeat-small-durable-candidate-6/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-1/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-2/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-3/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-4/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-5/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-baseline-6/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-1/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-2/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-3/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-4/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-5/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-04-wal-vectored-write/durable-candidate-6/run.json](../benches/results/2026-10-04-wal-vectored-write/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-durable-phase-refresh/process-1/run.json](../benches/results/2026-10-05-durable-phase-refresh/process-1/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-05-durable-phase-refresh/process-2/run.json](../benches/results/2026-10-05-durable-phase-refresh/process-2/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-05-durable-phase-refresh/process-3/run.json](../benches/results/2026-10-05-durable-phase-refresh/process-3/run.json) | complete | 2 | 48 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-baseline-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/durable-candidate-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-baseline-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/native-candidate-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/native-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/durable-candidate-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-baseline-6/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-1/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-1/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-2/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-2/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-3/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-3/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-4/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-4/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-5/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-5/run.json) | complete | 4 | 96 |
| [benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-6/run.json](../benches/results/2026-10-05-sparse-size-reserve/repeat/native-candidate-6/run.json) | complete | 4 | 96 |

## Полнота и происхождение

Все 136 текущих папок benches/results включены независимо от наличия отдельного отчёта. Все 158 Markdown-отчётов приведены с указанием источника; все найденные run.json индексированы. История Git по benches/results и docs/benchmark* не содержит удалённых файлов на момент сборки. Исторические цифры README (включая 2026-09-17) включены как опубликованные ранее результаты, без выдуманной повторной проверки. Логи, HTML, исходные снимки, скрипты, JSON-сводки, hashes и диагностические файлы остаются в исходных папках. Каталог не может восстановить несохранённые внешние/удалённые локальные замеры; они не объявляются выполненными. Даты исходных папок и UTC-время сообщений могут отличаться от местной даты чата.
