from pathlib import Path
import json,statistics,hashlib
p=Path(__file__).resolve().parent
for fn in ['checks.json','guard-build-status.json','compare-build-status.json','measurement-status.json','finish-status.json','durable-1k-recheck-status.json']:
 assert all(v==0 for v in json.loads((p/fn).read_text()).values()),fn
meta=json.loads((p/'provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert Path('src/exec.rs').read_bytes()==(p/'candidate-exec.rs').read_bytes()
d=json.loads((p/'medians.json').read_text());rd=json.loads((p/'rust-medians.json').read_text());g=json.loads((p/'guard-summary.json').read_text());rss=json.loads((p/'rss.json').read_text());check=json.loads((p/'durable-1k-recheck.json').read_text())
api=[];rust=[];guards=[];summary={'api':{},'rust':{}}
for series in [1,2]:
 for n in [1000,10000,100000]:
  k=f's{series}/{n}/';a=d[k+'lin'];sq=d[k+'sqlite'];du=d[k+'duckdb'];wins=sum(x<y for x,y in zip(a['candidate'],du['candidate']))
  api.append(f'| {series} | {n} | {a["paired_gain_percent"]:.3f}% ({a["wins"]}/6) | {a["candidate_median_ns"]/1e6:.6f} | {sq["candidate_median_ns"]/1e6:.6f} | {du["candidate_median_ns"]/1e6:.6f} | {wins}/6 | {sq["paired_gain_percent"]:.3f}% / {du["paired_gain_percent"]:.3f}% |')
  summary['api'][k]={'lin_vs_duckdb_process_wins':wins,'lin_candidate_ns':a['candidate_median_ns'],'duckdb_candidate_ns':du['candidate_median_ns']}
 for case in ['insert_bulk_1k','insert_bulk_10k','durable_insert_1k','durable_insert_10k']:
  k=f's{series}/compare/{case}/';a=rd[k+'lin'];b=rd[k+'sqlite'];wins=sum(x<y for x,y in zip(a['candidate'],b['candidate']))
  rust.append(f'| {series} | {case} | {a["paired_gain_percent"]:.3f}% ({a["wins"]}/6) | {b["paired_gain_percent"]:.3f}% | {a["candidate_median_ns"]/1e6:.6f} | {b["candidate_median_ns"]/1e6:.6f} | {wins}/6 |')
  summary['rust'][k]={'lin_vs_sqlite_process_wins':wins,'lin_candidate_ns':a['candidate_median_ns'],'sqlite_candidate_ns':b['candidate_median_ns']}
for k,v in g.items():guards.append(f'| {k} | {v["paired_gain_percent"]:.3f}% | {v["wins"]}/6 |')
r={v:statistics.median(x['max_rss_bytes'] for x in rss['observations'] if x['variant']==v) for v in ['baseline','candidate']}
summary['rss_median_bytes']=r
report='''# Retain: short-body hash cache bypass in large bulk insertion

Baseline source `20739a4`, production-identical to `311e11f`. Exact before/measured candidate/final exec.rs snapshots and API/guard/Rust-compare binary hashes retained. Measured source hashes verified unchanged after all runs. Final source differs by one explanatory comment only, recorded in provenance.json; no semantic/performance code changed after testing.

## Change and correctness

The 100k chunked profile measured ~28.6ms checks/defaults and ~31.4ms embedding. Short unique bodies churn the ~1023-entry TLS hash cache. The first, unconditional <=64-byte bypass regressed warm 1k cases and is rejected with all raw evidence in ../2026-10-24-short-body-hash-cache.

Only bulk default hashing now directly calls existing content_hash_arc at >=4096 statement records and <=64 UTF-8 body bytes. Smaller batches, longer bodies and updates retain the old cache. Explicit caller hashes remain unchanged. FNV/format/text values, default embedding/FTS/indexes, sync, quotas and wire semantics preserved. Thresholds are engineering heuristics, not universal crossovers.

Actual bulk unit coverage: 127/128/4095/4096/8192 records, unique/repeated bodies, 64/66-byte Unicode and caller hashes; stored hash values match the existing direct reference. Targeted test, release builds and full cargo test --workspace passed (finish-status.json). Profiling/candidate diagnostic binaries do not alter production configuration.

## Local native API measurements

Two independent series of six alternating process pairs at 1k/10k/100k, Lin/SQLite/DuckDB, 12 fresh samples each. <=10k single prepared run; 100k ten prepared chunks in one memory transaction. Setup/schema/preparation/exact six-field validation/cleanup/IPC excluded. Pinned binaries, no builds/tests initiated by this runner overlap timing; host activity otherwise uncontrolled. All runs complete/exact-validated, all outliers retained (including candidate 100k ~117ms and baseline DuckDB ~225ms in one pair). Rust versus Python driver/API feature costs differ; no isolated CPU, operation-p99 or equal-durability claim. See benchmark-contract.md.

| Series | Rows | Lin paired gain (wins vs baseline) | Candidate Lin ms | Candidate SQLite ms | Candidate DuckDB ms | Lin vs DuckDB processes | Control gains SQLite / DuckDB |
|---|---:|---:|---:|---:|---:|---:|---|
'''+ '\n'.join(api)+'''

100k aggregate medians beat DuckDB in both series; candidate wins 10/12 individual processes. Small API 1k effects remain slightly negative; no 1k acceleration claimed.

## Native Rust / durable checks

Fresh source-matched compare binaries; two independent series, six alternating process pairs per mode, 24 fresh samples/case, one operation, warmup 0, sample-ms 1. SQLite unchanged control; DuckDB excluded/MySQL skipped in this targeted check. Exact fixtures validated outside timers, same full sync and feature work. Process/batch medians are not operation p99.

| Series | Case | Lin paired gain (wins vs baseline) | SQLite control gain | Candidate Lin ms | Candidate SQLite ms | Lin vs SQLite processes |
|---|---|---:|---:|---:|---:|---:|
'''+ '\n'.join(rust)+f'''

Native 1k remains ahead of SQLite by aggregate median in both series and 11/12 processes, despite -2.584% first-series and -0.311% repeat changes versus baseline. Native/durable 10k each improve in all twelve baseline/candidate pairs; candidate durable 10k beats SQLite in 12/12 processes.

Durable 1k remains a loss. Its negative paired shifts, including -8.454% second-series, are retained. A separate concern-driven six-pair check with 48 samples/case gave Lin {check['lin']['paired_gain_percent']:.3f}% ({check['lin']['wins']}/6 wins), SQLite {check['sqlite']['paired_gain_percent']:.3f}% ({check['sqlite']['wins']}/6). Large -8.454% penalty did not reproduce, but this does not establish no regression or replace either original series. Direct bypass is inactive at 1k; CPU/layout and I/O variance remain practical limitations. No durable 1k gain claimed.

## Repeated-body and memory controls

Diagnostic worker changes only untimed body generation; repeated short text and repeated 256-byte text at 1k/10k. Six alternating pairs, 12 samples each, every row independently checked by Python. Long-body cache path unchanged. These Lin-only guards are not peer victories or universal no-regression proof.

| Body / rows | Paired gain | Candidate wins vs baseline |
|---|---:|---:|
'''+ '\n'.join(guards)+f'''

At 10k both short-repeat and unchanged long-repeat controls decrease similarly (~1.5%/~1.7%); no extra short-specific penalty isolated. Negative observations remain visible.

After timing, three additional alternating /usr/bin/time -l pairs on macOS, each whole-worker request 100k rows/12 fresh samples. Peak includes parsing/store/canonicalization/output and allocator effects, not insertion live memory. Median RSS: baseline {r['baseline']/1024**2:.3f} MiB, candidate {r['candidate']/1024**2:.3f} MiB (+{(r['candidate']/r['baseline']-1)*100:.2f}%). Candidate higher in 3/3 pairs; observed memory tradeoff, no causal allocator attribution or memory-saving claim.

## Decision / remaining scope

Retain for repeated native large-batch and durable 10k gains, acknowledging small-case negative shifts and higher whole-worker RSS. Correctness/workspace gates passed. Local aggregate native 100k DuckDB and Rust durable 10k SQLite gaps close within these finite repeated contracts. Durable 1k gap remains. Full read/server-native matrices need refreshing on accepted source before broadening claims. PostgreSQL/MySQL/MongoDB not rerun here; MSSQL/Kusto endpoints/native ingestion remain unqualified. Pandas construction is a separate nontransactional contract. Full named-peer objective stays open.
'''
# Comment-only final edit; runtime and tested source remain represented exactly.
f=Path('src/exec.rs');s=f.read_text();a='// Use precomputed hash from cache to avoid repeated FNV-1a computations';b='// Large short-body batches bypass the cache; small batches retain reuse.'
assert s.count(a)==2
f.write_text(s.replace(a,b,1));(p/'final-exec.rs').write_bytes(f.read_bytes())
meta.update(final_exec_sha256=hashlib.sha256(f.read_bytes()).hexdigest(),final_comment_only_change={'before':a,'after':b})
(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n');(p/'overview.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'report.md').write_text(report)
f=Path('docs/benchmark-history.md');s=f.read_text();needle='Исторический каталог ниже сохраняет срез 5 октября. Новые измерения текущего кода:\n\n';assert needle in s
s=s.replace(needle,needle+'- [Large-bulk short-body hashing](../benches/results/2026-10-24-bulk-short-body-hash/report.md): повторно закрыты агрегированные native 100k DuckDB и Rust durable 10k SQLite gaps. Сохранены небольшие отрицательные эффекты 1k и рост whole-worker RSS ~4%; durable 1k проигрывает. [Безусловный обход кэша отклонён](../benches/results/2026-10-24-short-body-hash-cache/report.md).\n',1);f.write_text(s)
for root in [p,p.parent/'2026-10-24-short-body-hash-cache']:
 for f in root.rglob('*.log'):f.write_text(f.read_text().rstrip()+'\n')
print('report and final comment written')
