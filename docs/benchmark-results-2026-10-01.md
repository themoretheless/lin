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
