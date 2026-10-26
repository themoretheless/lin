# Prepared index-covered fixed-text-group COUNT — accepted

Frozen baseline`f19f41a4`, macOS ARM64, compiler/source/binary manifests in
`provenance.json`. Initial Linux CI observation belongs to97f54abd; its timings
are not pooled with this local experiment. The baseline was rebuilt from exact
before-exec.rs; accepted small durable WAL dispatch is present in both variants.

## Change and functional checks

Prepared filtered grouped COUNT bypassed existing index-only logic and scanned
row maps in execute_agg_plan. The new narrow path counts index postings only when
the index fully covers the predicate and equality fixes the sole text group.
Empty matches return no group. A nonempty group enforces the exact generic
Cell::Text aggregate-state quota (UTF-8 byte length+32+64). Residual predicates,
nonfixed groups, numeric/null groups and other aggregates keep existing paths.
No result cache, new index, schema, embedding, FTS, durability or row format change.
This does not repair every pre-existing ordinary/grouping-helper quota path.

Five integration tests verify explicit expected groups in mutable/readonly APIs,
range and residual predicates, contradictory equalities, multiple groups,
updates/deletes/rollback, exact99-byte quota boundary, zero-match at quota0,
Unicode byte counts and empty keys. An independently built10k-row default-embedder
fixture with two composite index keys and1-day/30-day timestamps checks both timed
query shapes return exactly5000. Workspace/no-default/optional GPU+parallel+
compression checks passed. Final release binary equals the measured candidate
SHA256 after test-only additions. Tests establish correctness, not acceleration.

## Full-engine comparison

Two independent series x6 alternating baseline/candidate process pairs, warm
fixtures,24 wall samples/case, max1024 ops/batch,5ms warmup,1ms target sample.
Families: grouped equality/range, point get, substring and inner join.96 A/B
processes. Lin/SQLite/DuckDB run in each applicable process; PostgreSQL/MySQL
explicitly disabled rather than counted as wins. Prepared/API contract unchanged;
setup/fixture borrow/drop excluded, output destruction included. The count result
is a single group; this does not generalize to arbitrary group cardinalities.
Input timestamps/index shape are validated by the benchmark's fixture checks;
result correctness is additionally checked by the independent tests above.
No own builds overlap timing; every raw observation/control/load is retained.

| Case | Baseline us s1 / s2 | Candidate us s1 / s2 | Paired gain s1 / s2 (%) | Positive pairs |
|---|---:|---:|---:|---:|
| filter_eq | 532.698 / 536.422 | 0.864 / 0.864 | +99.837 / +99.838 | 6/6 / 6/6 |
| filter_range | 683.604 / 689.604 | 1.432 / 1.438 | +99.791 / +99.793 | 6/6 / 6/6 |
| point_get | 0.120 / 0.113 | 0.118 / 0.119 | +2.923 / -3.391 | 5/6 / 1/6 |
| text_substr | 34.854 / 34.731 | 35.159 / 34.688 | +0.123 / -0.208 | 3/6 / 3/6 |
| join_inner | 149.650 / 143.913 | 147.702 / 148.604 | +0.575 / -1.717 | 4/6 / 1/6 |

Both target cases win all12 baseline/candidate pairs each. The candidate wins
all48 process-level target comparisons against SQLite/DuckDB. Minimum within-
process speedup over either peer is45.55x. These are two10k warm API workloads,
not isolated engine CPU measurements or universal superiority.

## Controls and resources

Point-get changes reverse sign across series(+2.92/−3.39%); SQLite point control
is−0.90/−3.41%. Lin join is+0.57/−1.72%; substring+0.12/−0.21%. Do not present these
as secondary optimizations. Additional A/A runs execute the exact same frozen
baseline binary under both labels:48 processes, same samples/calibration.

| Control | A/A signed median s1 / s2 (%) | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|
| point_get | -1.255 / +1.535 | 3.666 / 2.886 |
| join_inner | +0.044 / +0.082 | 1.565 / 0.560 |

Six separate process-resource probes use one equality operation with the same10k fixture. Peak RSS includes fixture/startup and is not aggregate-state memory:

- baseline: median80.84MiB, range80.80–81.08MiB.
- candidate: median81.59MiB, range81.16–82.55MiB.

Accept the repeated target gain: approximately616–621x equality and477–479x range,
with correct results and quota semantics. Controls show small host/process
variation; no adjustment or sample rejection is applied. A/A controls are not
used to claim every unrelated path is unchanged. New platform CI is still needed
to confirm the Linux observation is repaired; its separate join gate remains
unresolved. The old CI budget mixes historical macOS absolute limits with Linux.
Do not raise thresholds to hide it.

MSSQL is qualified separately on frozen8b0e9717 with private results; this change
was not measured against it. Real Kusto endpoint and SQLite durable1k gap remain
open. The full eight-peer goal is not complete.

`verify.py` validates saved hashes, sample counts/medians/statuses and peer wins,
not a new runtime run. Rebuild scripts swap frozen source files: use an isolated
worktree, never a current/dirty checkout. Raw data and all negative controls stay
available. `diagnosis.md` preserves the pre-change CI interpretation.
