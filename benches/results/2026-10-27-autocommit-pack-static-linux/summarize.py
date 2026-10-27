from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;status=json.loads((p/'process-status.json').read_text());assert len(status)==720 and all(r['exit_code']==0 for r in status)
assert len({(r['round'],r['mode'],r['case'])for r in status})==720
allrows={};raw_count=0
for i in range(1,13):
 groups={}
 for mode in ['baseline','control','candidate']:
  group={}
  for record in [r for r in status if r['round']==i and r['mode']==mode]:
   case=record['case'];d=json.loads((p/f'{i:02}-{mode}'/case.replace('/','_')/'run.json').read_text())
   assert d['status']=='complete' and len(d['cases'])==1 and len(d['observations'])==8
   vals=[]
   for o in d['observations']:
    assert o['case']==case and o['metric']=='wall' and o['operations']==1
    value=float(o['value']);assert math.isfinite(value)and value>0;vals.append(value);raw_count+=1
   group[case]=st.median(vals)
  assert len(group)==20;groups[mode]=group
 assert set(groups['baseline'])==set(groups['control'])==set(groups['candidate'])
 for case,a in groups['baseline'].items():
  b=groups['control'][case];c=groups['candidate'][case];r=dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a))
  if case.endswith('/lin'):
   r['candidate_sqlite_ns']=groups['candidate'][case[:-3]+'sqlite'];r['wins_sqlite']=c<r['candidate_sqlite_ns']
  allrows.setdefault(case,[]).append(r)
assert raw_count==5760
out={}
for case,rows in allrows.items():
 out[case]={}
 for label,g in [('series1',rows[:6]),('series2',rows[6:])]:
  q=dict(pairs=len(g),gain_percent=st.median(r['gain_percent']for r in g),aa_absolute_percent=st.median(abs(r['aa_percent'])for r in g),positive_pairs=sum(r['gain_percent']>0 for r in g),processes=g)
  if case.endswith('/lin'):q['sqlite_wins']=sum(r['wins_sqlite']for r in g)
  out[case][label]=q
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
print('720 isolated processes; 5760 observations; 240 paired comparisons')
