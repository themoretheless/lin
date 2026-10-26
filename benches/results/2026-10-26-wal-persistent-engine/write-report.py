from pathlib import Path
import json
r=Path(__file__).resolve().parent
m=json.loads((r/'rust-medians.json').read_text());inter=json.loads((r/'interleaved-summary.json').read_text());boundary=json.loads((r/'boundary-summary.json').read_text())
for rows,data in [(10000,inter),(4096,boundary),(16384,boundary)]:
 for suffix in ['', '-s2']:
  aa=next(x for x in data if x['mode']=='aa'+suffix and x['rows']==rows);ab=next(x for x in data if x['mode']=='ab'+suffix and x['rows']==rows)
  assert ab['wins']==6 and ab['gain']>aa['median_abs']
lines=['# Shared persistent WAL encoder — full-engine qualification', '',
'2026-10-26. Frozen baseline `53ebe9b`; private managed worktree and APFS-cloned build caches. Production source changes are confined to `exec.rs`, `persist.rs`, `store.rs`. This is a different mechanism and eligibility policy from the rejected scoped-thread port. It does not retroactively qualify that earlier prototype.', '',
'## Mechanism and correctness', '',
'Shared bounded queue (capacity1), lazy per-store leases and join on last lease; rows move through `Arc<Vec<Row>>` without deep row-map copying for transfer. The worker releases the input Arc before reply; caller recovers the original Vec. Queue-full/disconnected/unavailable worker falls back to serial. Encoder panics are caught so the worker survives, then deferred to the original commit point alongside encode errors; index/quota error precedence is preserved. Main thread retains file/framing/checksum/Full sync. Original logical Pack and WAL codec remain unchanged.', '',
'Policy: single ordinary durable insert, no edges,4096..=16384 rows,2Mi..=16Mi finalized embedding floats. Native,1k,scalar-only,explicit transactions,multi-statements and outside-window batches remain sequential. See [design](../../../docs/wal-encoder-worker.md).', '',
'Default workspace passed. Focused tests cover readonly replay, error/quota/index precedence, rollback without WAL changes, metadata mismatch, byte-equality with the original encoder (including special floats/empty/dense/parallel sparse), and noneligible paths. Added two-database worker sharing, reuse after caught encoder panic, actual full/disconnected queue ownership/encoding, and last-lease join tests passed. OS thread-creation failure is simulated as unavailable-worker fallback; no real OS resource exhaustion was forced. Arbitrary panic rollback is not promised.', '',
'No-default lib check and GPU/parallel/compression feature compilation passed; this is not GPU runtime proof. After adding queue/lifecycle tests, both release binaries rebuilt **byte-identically** to the measured candidate. Final source hashes and snapshots are retained.', '',
'## Conventional alternating processes', '',
'Two series ×6 alternating baseline/candidate process pairs per native/Full durable mode:48 processes,4608 observations.24 samples/case,one operation,no warmup. Fresh setup/preparation/readback/output destruction outside timing; embeddings/FTS/indexes and Full sync retained. SQLite controls unchanged; DuckDB excluded from this particular insertion qualification. Starting host load1:7.77–12.64, not CPU utilization. Every process exits0; all observed times finite/positive and operations==1. No own builds/tests overlap timing.', '',
'Gain below is median of paired process-median ratios. Candidate latency columns are medians of process medians, not the ratio used to compute gain.', '',
'| Series | Case | Lin gain /positive pairs | SQLite control gain | Candidate Lin ms | Candidate SQLite ms |',
'|---:|---|---:|---:|---:|---:|']
for series in [1,2]:
 for case in ['insert_bulk_1k','insert_bulk_10k','durable_insert_1k','durable_insert_10k']:
  a=m[f's{series}/compare/{case}/lin'];b=m[f's{series}/compare/{case}/sqlite'];lines.append(f"| {series} | {case} | {a['paired_gain_percent']:+.3f}% /{a['wins']}/6 | {b['paired_gain_percent']:+.3f}% | {a['candidate_median_ns']/1e6:.3f} | {b['candidate_median_ns']/1e6:.3f} |")
lines+=['','Durable10k candidate beats the same-process SQLite measurement in6/6 processes in both series; median paired SQLite/Lin speedup1.233× /1.245×. Durable1k loses SQLite in all12 candidate processes. Native gains are mixed between series and are not attributed to the worker, which is disabled there.', '',
'## Interleaved A/A and A/B', '',
'Two independently executed series for1k/10k and for dispatch boundaries4096/16384;6 pairs/size/series,24 samples/worker plus a preflight.192 worker processes,4608 measured operations plus192 preflights; exact nine-field readback including f32 bits after every operation. A/A uses byte-identical baseline copies. Each request opens a fresh durable Db: the eligible measured call includes cold worker creation; close/join/fixture cleanup are outside timing. All workers exit0 and owned fixture cleanup assertions pass. Source/binary hashes retained and source drift checks pass.', '',
'| Rows | Series | A/B gain | Positive pairs | A/A signed gain | A/A absolute difference |',
'|---:|---:|---:|---:|---:|---:|']
for data in [inter,boundary]:
 for rows in sorted({x['rows'] for x in data}):
  for series,suffix in [(1,''),(2,'-s2')]:
   aa=next(x for x in data if x['mode']=='aa'+suffix and x['rows']==rows);ab=next(x for x in data if x['mode']=='ab'+suffix and x['rows']==rows)
   lines.append(f"| {rows} | {series} | {ab['gain']:+.3f}% | {ab['wins']}/6 | {aa['gain']:+.3f}% | {aa['median_abs']:.3f}% |")
lines+=['','All negative controls/outliers are retained.1k A/B changes sign between series alongside A/A dispersion; no1k improvement or causal slowdown is claimed. Stable10k and boundary gains exceed observed A/A dispersion in both series. This is a finite fixture qualification, not proof for every corpus/vector density/scalar width/platform.', '',
'## Process-exit replay and resources', '',
'For1k/10k and1/3/4 commits, baseline/candidate exit via `process::exit(0)` without database destructors.12 writer processes plus12 independent readonly verifiers recover every acknowledged row with exact embedding bits. WAL bytes match baseline/candidate in all6 scenarios. This checks process-exit recovery, not physical power removal.', '',
'One `/usr/bin/time -l` RSS observation per variant/scenario:1k four commits32.31/32.47MiB baseline/candidate;10k one commit84.34/83.27MiB;10k four commits273.83/275.27MiB. Raw results retained. These single observations do not establish statistical RSS equivalence or a universal memory bound. Repeated commits exercise reuse; two-store and last-lease tests cover lifecycle, not a large-client contention performance campaign.', '',
'## Decision', '',
'**Accept the shared persistent-worker policy for the tested durable batch range.** Full-engine10k gain reproduces in both conventional and interleaved series, dispatch boundaries pass two A/A/B series, exact bytes/replay/error/lifecycle checks pass, and resource observations reveal no large default-fixture growth. This acceptance is separate from the full eight-peer objective.', '',
'Full SQLite1k gap remains open. No new read/native matrix versus DuckDB/PostgreSQL/MySQL/MongoDB/pandas is claimed by this report; previous matrices remain historical until refreshed. Endpoint-presence booleans for MSSQL/Kusto are false; no Microsoft server win is claimed. No sync weakening, feature disabling, new writer codec, or discarded losing observation is used to obtain the result.', '']
(r/'report.md').write_text('\n'.join(lines))
