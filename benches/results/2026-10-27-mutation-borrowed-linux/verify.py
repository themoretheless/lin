from pathlib import Path
import json,statistics as st,hashlib,math
p=Path('benches/results/2026-10-27-mutation-borrowed-linux');a=p/'artifact';status=json.loads((a/'status.json').read_text());assert len(status)==36 and all(x['exit_code']==0 for x in status);summary=json.loads((a/'summary.json').read_text());total=0
for case,series in summary.items():
 for label,q in series.items():
  assert q['pairs']==6
  for r in q['processes']:
   group={}
   for mode in ['baseline','control','candidate']:
    d=json.loads((a/f"{r['round']:02}-{mode}"/'run.json').read_text());assert d['status']=='complete' and len(d['cases'])==20 and len(d['observations'])==160
    vals=[float(o['value'])/o['operations'] for o in d['observations']if o['case']==case and o['metric']=='wall'];assert len(vals)==8 and all(math.isfinite(x)and x>0 for x in vals);assert all(o['operations']==1 for o in d['observations']);group[mode]=st.median(vals)
   assert group['baseline']==r['baseline_ns'] and group['candidate']==r['candidate_ns'] and group['control']==r['control_ns'];total+=1
assert total==240
peers=sum(q['sqlite_wins'] for case,v in summary.items()if case.endswith('/lin')for q in v.values());assert peers==0
(p/'verification.json').write_text(json.dumps(dict(completed_process_runs=36,cases_per_run=20,samples_per_case=8,timed_observations=5760,recomputed_case_pairs=240,candidate_sqlite_wins=0,candidate_sqlite_comparisons=120,candidate_qualified=False,production_enabled=False,initial_verifier_error='Raw value is a numeric string; initial direct division failed before any conclusions. Corrected float conversion, raw artifacts unchanged.'),indent=2)+'\n')
(p/'report.md').write_text('''# Borrowed-row candidate: Linux qualification not accepted

Run40394312683 on frozen6ac970ef completed all36 benchmark processes: two six-round series, rotated A/A/B,20native mutation/transaction cases including SQLite,8samples/case,5760timed operations. Candidate test/contract checks passed; raw benchmark validation, full source snapshots and binary hashes retained. Independently recomputed all240case-pairs from raw samples. No observations discarded.

| Lin target | Series1 gain / pairs / A-A | Series2 gain / pairs / A-A |
|---|---|---|
| update1k |+6.005% /5of6 /0.984%|+7.037% /6of6 /2.146%|
| update10k |+4.298% /4of6 /1.006%|+1.972% /5of6 /1.523%|
| update100k |+3.797% /5of6 /3.065%|+4.213% /4of6 /3.176%|

The predeclared gate requires6of6 target wins exceeding A/A in both series and unchanged controls within its bounds. It returned candidate_qualified=false. For example, delete100k series2 regressed2.533% versus A/A2.230%; SQLite transaction rollback10k series1 regressed6.807% versus A/A2.573%. Full table is artifact/paired-report.md, all controls/losses preserved.

All120 candidate same-case comparisons against SQLite lose. Reduced allocations are corroborated independently but do not close the actual objective. No production rollout, no accepted full-durable speedup, no eight-peer superiority claim. The previous local noisy attempt and initial ANSI harness failure remain separate retained experiments.

Next mechanism: prototype30ee3dd6 additionally transfers the original Row into undo ownership (30allocations saved rather than20), with278tests and54allocation-counter operations already passed. Qualify it separately; do not reinterpret or weaken this failed gate.
''')
hashes={str(f.relative_to(p)):hashlib.sha256(f.read_bytes()).hexdigest()for f in p.rglob('*')if f.is_file()and f.name!='sha256.json'};(p/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n');print('verified all240pairs,5760timed operations,0/120SQLite wins')
