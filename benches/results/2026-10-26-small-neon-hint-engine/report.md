# Conservative small-batch work hint — accepted

2026-10-26. Frozen baseline `8b0e9717` (including the stable borrow-check fix),
with source/compiler/binary manifests. Independent experiment from the rejected
256Ki and512Ki floor-only variants. No observations from those variants are
pooled into this acceptance.

## Change and scope

Single implicit durable inserts with512..4095 rows may use the existing shared
encoder when the first finalized embedding length times row count is at least
512Ki float values, and the exact finalized total is512Ki..16Mi floats. The hint
is O(1); false negatives stay serial.4096..16384 retains the old exact2Mi..16Mi
float policy. No edges; native, explicit-transaction and multi-statement guards
remain unchanged. Floats are counts, not bytes (512Ki floats is2MiB raw f32).
The work hint avoids an O(n) total scan in clearly ineligible small slabs.

No codec, framing, CRC, sync, FTS/index, quota, returned values or ordering change.
Full durability remains enabled. Existing worker/lease/queue ownership is reused.
Concurrent-database throughput and other hardware are not performance-qualified.

## Functional gates

Workspace, no-default and optional GPU/parallel/compression checks passed before
and after final test-only additions. Six focused worker tests pass. Coverage:
683/1k/4096 replay/fallback; exact512x1024 float budget; small/large dimensional
guards; quota/index precedence; spawn/queue/disconnect fallback; encoder failures;
explicit transactions and multi-statement serial execution. New mixed-vector
oracle checks a misleading positive hint, a conservative negative hint and a
true positive. All683 rows retain dimensions and raw signed-zero/subnormal bits
in memory and independent readonly replay.

24 process-exit writers +24 independent verifiers cover512/683/1k/10k x1/3/4
commits. All12 baseline/candidate WAL files byte-identical; all rows/bits correct.
These are process-exit tests, not physical power-loss proof. RSS/WAL sizes saved.
After the new tests, both release compare and interleaved binary hashes match
those measured; original candidate snapshots and post-test source are retained.

## Conventional full-engine measurements

Two series x6 alternating process pairs/native+Full durable,24 fresh samples per
case:48 processes/4608 observations. Embeddings, FTS, scalar indexes and Full sync
preserved. Setup/preparation/validation and destruction excluded. SQLite controls,
load and every process retained. No own build overlaps timing.

| Case | Gain s1 / s2 (%) | Positive pairs s1 / s2 | Candidate ms s1 / s2 |
|---|---:|---:|---:|
| compare/durable_insert_10k/lin | +0.309 / -1.990 | 4/6 / 1/6 | 10.659 / 10.520 |
| compare/durable_insert_10k/sqlite | +0.797 / -0.151 | 6/6 / 2/6 | 13.485 / 13.355 |
| compare/durable_insert_1k/lin | +10.981 / +11.737 | 6/6 / 6/6 | 1.638 / 1.733 |
| compare/durable_insert_1k/sqlite | -0.788 / -0.390 | 1/6 / 2/6 | 1.365 / 1.369 |
| compare/insert_bulk_10k/lin | -1.109 / -1.203 | 2/6 / 2/6 | 8.102 / 8.422 |
| compare/insert_bulk_10k/sqlite | +0.389 / +0.418 | 4/6 / 3/6 | 10.337 / 10.443 |
| compare/insert_bulk_1k/lin | +0.793 / -3.354 | 3/6 / 1/6 | 0.760 / 0.798 |
| compare/insert_bulk_1k/sqlite | +0.554 / -0.046 | 3/6 / 3/6 | 0.790 / 0.796 |

## Interleaved repeats and controls

Each cell below has6 process pairs x24 alternating samples per variant. A/A
uses byte-identical baseline binaries. Fresh DB per operation, cold worker start
inside timing, final lease join/Drop/cleanup outside timing. Independent exact
readback after each sample. Main/boundary/native families:6912 observations +288
preflights. A targeted serialguard follow-up adds2304 observations +96 preflights;
this was run because682 was initially inconclusive. It does not replace initial
negative samples. Native protocol and conventional results remain separate.

| Family / rows | A/B gain s1 / s2 (%) | Wins s1 / s2 | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| interleaved / 1000 | +13.280 / +10.153 | 6/6 / 6/6 | 2.695 / 2.374 |
| interleaved / 10000 | -0.510 / -0.424 | 3/6 / 2/6 | 1.744 / 2.055 |
| boundary / 682 | -1.448 / -1.944 | 2/6 / 2/6 | 4.258 / 1.154 |
| boundary / 683 | +9.795 / +11.786 | 6/6 / 6/6 | 2.398 / 2.677 |
| native / 1000 | +0.042 / +0.543 | 3/6 / 3/6 | 0.768 / 1.226 |
| native / 10000 | +1.155 / +0.836 | 6/6 / 5/6 | 0.623 / 0.540 |
| serialguard / 512 | -0.720 / +1.660 | 1/6 / 5/6 | 2.460 / 1.755 |
| serialguard / 682 | +0.212 / +2.841 | 3/6 / 6/6 | 3.098 / 2.903 |

The first serialguard attempt used511, unsupported by the frozen harness. It
failed an input assertion before preflight/timing; retained in
`serialguard-initial-unsupported-size/`. Corrected512/682 probes use exactly the
same measured binaries. No discarded timing observations.

## Acceptance and remaining gaps

Accept the small-batch dispatch: durable1k improves in all12 conventional pairs
and all12 interleaved pairs, beyond contemporaneous A/A variation. Entry683 also
wins12/12. Initial682 medians were−1.45/−1.94%; additional independent guard repeats
gave+0.21/+2.84% amid A/A variation. No repeatable guard regression is established.
Conventional native1k second series−3.35% is retained; targeted native interleaved
repeats+0.04/+0.54% did not reproduce it. Do not claim native acceleration or a
universal absence of regressions.10k policy is unchanged and its small negative
interleaved medians are inside measured A/A variation.

Candidate durable1k is1.638/1.733ms against SQLite1.365/1.369ms: **still loses**.
Durable10k is10.659/10.520ms against SQLite13.485/13.355ms. Native1k is0.760/0.798ms
against SQLite0.790/0.796ms: no stable win over SQLite is proved. This experiment
does not measure MSSQL/Kusto or refresh the complete available-peer matrix.
The full eight-peer objective remains incomplete.

`verify.py` checks stored snapshots, samples, medians, statuses and WAL evidence;
it is not a fresh runtime/performance run. Full raw data and negative controls
are preserved; `final-checks.json` ties the final test-only source to identical
measured release binaries.

Rebuild scripts swap whole frozen source files. Run them only in a dedicated
isolated worktree, never over a current/dirty checkout. Original measured
snapshots precede the final test-only additions; reproduce those exact snapshots
when comparing binary hashes. The verifier itself only reads stored evidence.
