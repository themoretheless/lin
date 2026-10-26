from pathlib import Path
import json,statistics as st,importlib.util
p=Path(__file__).resolve().parent;root=p.parents[2];spec=importlib.util.spec_from_file_location('budget',root/'scripts/check-bench-budget.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
allrows={}
for i in range(1,13):
 group={}
 for mode in ['baseline','control','candidate']:
  f=p/f'{i:02}-{mode}'/'run.json'
  if not f.exists():break
  d=json.loads(f.read_text())
  if d['status']!='complete':break
  assert len(d['cases'])==20 and len(d['observations'])==160
  assert all(o['operations']==1 for o in d['observations'])
  group[mode]=module.load_medians(d)
 if len(group)!=3:continue
 for case,a in group['baseline'].items():
  b=group['control'][case];c=group['candidate'][case]
  row=dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a))
  if case.endswith('/lin'):
   peer=case[:-3]+'sqlite';row['candidate_sqlite_ns']=group['candidate'][peer];row['wins_sqlite']=c<group['candidate'][peer]
  allrows.setdefault(case,[]).append(row)
out={}
for case,rows in allrows.items():
 out[case]={}
 for series,group in [('series1',[r for r in rows if r['round']<=6]),('series2',[r for r in rows if r['round']>6])]:
  if not group:continue
  q=dict(pairs=len(group),gain_percent=st.median(r['gain_percent']for r in group),aa_absolute_percent=st.median(abs(r['aa_percent'])for r in group),positive_pairs=sum(r['gain_percent']>0 for r in group),processes=group)
  if case.endswith('/lin'):q['sqlite_wins']=sum(r['wins_sqlite']for r in group)
  out[case][series]=q
 print(case,' '.join(f"{name}:{q['gain_percent']:+.2f}% A/A{q['aa_absolute_percent']:.2f}% {q['positive_pairs']}/{q['pairs']}"for name,q in out[case].items()))
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
print('complete_rounds',len(next(iter(allrows.values()),[])))
