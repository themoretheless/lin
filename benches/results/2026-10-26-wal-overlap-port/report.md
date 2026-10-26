# WAL encoding/maintenance overlap port — full-engine qualification

2026-10-26, frozen baseline e7abf40. Local execution resumed: previous build/diagnostic handles no longer existed and process inspection found no Lin build. Old build output had stopped at compilation; there is no old release/benchmark evidence to reuse. Historical prototype snapshots were three-way ported to this newer baseline, preserving the committed security, transaction and FTS changes. Text merge had no conflicts; candidate full workspace subsequently passed.

Qualification used a newly attached isolated worktree and private APFS copy-on-write build caches. The older dirty profile worktree and primary checkout source were untouched. Concurrent uncommitted primary changes are not included or qualified here. Source/binary hashes, compiler, exact baseline/candidate snapshots and build/test/runner logs retained. This is a finite qualification of the pinned e7abf40-derived candidate, not of arbitrary later main changes.

## Prototype behavior and checks

Pure typed-column encoder returns existing codec/payload, without file I/O. Eligible ordinary single durable inserts stage the owned result while caller maintains indexes/FTS/maps; caller writes/checksums/syncs at the original commit point. Logical Pack serialization remains unchanged. Eligibility:512..4095 rows, no edges, finalized embedding float count256Ki..4Mi. Native, small, scalar-only, explicit-transaction and multi-statement paths remain sequential;10k also stays sequential. Their compiled code still changes through refactoring, so they require regression observation.

Full default workspace passed. Tests cover original-frame equality including special floats/empty/dense/parallel sparse columns, no-file-write decoded-budget rejection, actual worker-create failure injection, readonly replay, deferred encode errors/panics preserving quota/index error precedence, ordinary failure rollback/no WAL change, noneligible paths, and generation/next-id/collection/field/count mismatch fallback. These are functional tests, not physical power-loss or universal platform proof. Current prototype has no peak-RSS/cold/repeated-commit resource qualification and is not accepted.

## Conventional process-pair results

Two series ×six alternating fresh process pairs per mode; native/Full durable1k/10k,24 samples/case/process, one operation, no warmup. Setup/preparation/readback/drop outside timing; embedding, FTS, indexes and sync preserved.48 processes complete, every measured value finite/positive and operations==1. Unchanged SQLite controls. Runner-initiated builds/tests completed before measurement, but other host tasks were active. Recorded load1 ranged20.74..30.04; load is not CPU usage or proof of a specific cause. All observations/outliers retained.

| Case | Engine | Series1 paired gain /positive pairs | Series2 paired gain /positive pairs |
|---|---|---:|---:|
| insert_bulk_1k | lin | +18.262% /5/6 | -0.193% /2/6 |
| insert_bulk_1k | sqlite | +2.404% /4/6 | -0.717% /3/6 |
| insert_bulk_10k | lin | -20.473% /1/6 | +0.821% /4/6 |
| insert_bulk_10k | sqlite | -12.508% /1/6 | -0.077% /3/6 |
| durable_insert_1k | lin | +10.574% /4/6 | -27.033% /3/6 |
| durable_insert_1k | sqlite | -1.591% /1/6 | -30.464% /3/6 |
| durable_insert_10k | lin | -19.320% /1/6 | -10.641% /2/6 |
| durable_insert_10k | sqlite | -9.078% /2/6 | -4.224% /3/6 |

Durable1k first series positive, second negative alongside a strongly shifting SQLite control. Both durable10k series negative. Candidate1k medians2.406/3.732ms lose SQLite1.853/2.369ms;10k26.439/24.945ms lose SQLite16.632/24.434ms. No control-adjusted or causal gain is asserted.

## Interleaved A/A and A/B gate

Fresh persistent worker binaries, six pairs per size,24 alternating samples/worker; each request uses fresh Full Db and exact nine-field/f32-bit reference validation outside timing. Both source snapshots and compiler recorded. A/A uses byte-identical baseline copies. Worker/fixture lifecycle differs from conventional compare: protocols are not pooled.1152 measured operations plus48 preflights;48 worker processes exit zero and owned fixture directories are removed. No runner builds/tests overlap measurements; host load remains uncontrolled.

| Comparison | Rows | Median paired gain | Median absolute pair difference | Positive pairs |
|---|---:|---:|---:|---:|
| aa | 1000 | +5.959% | 12.233% | 4/6 |
| aa | 10000 | -0.522% | 2.780% | 2/6 |
| ab | 1000 | -8.713% | 10.416% | 1/6 |
| ab | 10000 | +1.797% | 6.659% | 3/6 |

## Decision: not accepted; preserve prototype, do not merge source

The component CPU gate did not establish full commit improvement. Full-engine interleaved1k loses in5/6 pairs; identical-binary A/A itself has substantial dispersion. This does not isolate thread-startup, encoding, code layout, filesystem or host scheduling as a cause; do not claim a causal regression solely from these noisy series. It does establish insufficient evidence to retain this scoped-thread policy. Keep every conventional loss and A/A outlier; no retry-until-win or peer refresh for this unchanged unqualified candidate.

Isolated source is restored to its frozen baseline after qualification; candidate snapshots remain reviewable. No production source is merged into main. No new codec, disabled work or relaxed durability. No actual process-exit crash/resource/optional-platform campaign run for this port beyond workspace tests; those would remain required for any future accepted implementation.

Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas objective remains active and unmet. Durable SQLite gap persists. Endpoint presence is recorded as booleans only; no live Microsoft peer win is claimed. Historical108-read/15-native scenario wins are not fresh measurements of this candidate.
