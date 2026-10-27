from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;status=json.loads((p/'status.json').read_text());assert len(status)==36 and all(r['exit_code']==0 for r in status)
cases={f'compare/{kind}_{size}/{engine}'for kind in ['insert_bulk','durable_insert']for size in ['1k','10k']for engine in ['lin','sqlite']};allrows={};observations=0
for i in range(1,13):
 groups={}
 for mode in ['baseline','control','candidate']:
  data=json.loads((p/f'{i:02}-{mode}'/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==8 and len(data['observations'])==64
  assert {o['case']for o in data['observations']}==cases;groups[mode]={}
  for case in cases:
   samples=[]
   for o in data['observations']:
    if o['case']!=case:continue
    assert o['metric']=='wall'and o['operations']==1;v=float(o['value']);assert math.isfinite(v)and v>0;samples.append(v);observations+=1
   assert len(samples)==8;groups[mode][case]=st.median(samples)
 for case in cases:
  a,b,c=(groups[m][case]for m in ['baseline','control','candidate']);r=dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a))
  if case.endswith('/lin'):r.update(sqlite_ns=groups['candidate'][case[:-3]+'sqlite'],wins_sqlite=c<groups['candidate'][case[:-3]+'sqlite'])
  allrows.setdefault(case,[]).append(r)
assert observations==2304
out={};accepted=True
for case,rows in sorted(allrows.items()):
 out[case]={}
 for label,group in [('series1',rows[:6]),('series2',rows[6:])]:
  gain=st.median(r['gain_percent']for r in group);noise=st.median(abs(r['aa_percent'])for r in group);positive=sum(r['gain_percent']>0 for r in group);passed=(positive==6 and gain>noise)if case.endswith('/lin')else(gain>=-max(2,noise));accepted &= passed
  r=dict(gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,processes=group)
  if case.endswith('/lin'):r['sqlite_wins']=sum(v['wins_sqlite']for v in group)
  out[case][label]=r;print(case,label,round(gain,3),round(noise,3),positive,passed,r.get('sqlite_wins',''))
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(dict(candidate_qualified=accepted,full_objective_achieved=False,processes=36,observations=2304),indent=2)+'\n')
