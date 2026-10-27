from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;status=json.loads((p/'status.json').read_text());assert len(status)==48 and all(r['exit_code']==0 for r in status);med={};layouts={};observations=0
for r in status:
 rows=[json.loads(l)for l in(p/f'{r["round"]:02}-{r["role"]}.jsonl').read_text().splitlines()];assert len(rows)==1321;layouts[r['role']]=rows[0];cases={}
 for o in rows[1:]:
  assert o['iterations']in [64,2048]and o['elapsed_ns']>0 and o['warmup']==(o['sample']<2)
  if not o['warmup']:cases.setdefault(o['case'],[]).append(o['elapsed_ns']/o['iterations']);observations+=1
 assert len(cases)==132 and all(len(v)==8 for v in cases.values())
 for case,v in cases.items():med[r['round'],r['role'],case]=st.median(v)
assert observations==50688;summary={}
for case in cases:
 summary[case]={}
 for label,rounds in [('series1',range(1,7)),('series2',range(7,13))]:
  pairs=[]
  for i in rounds:
   a,b,c=[med[i,m,case]for m in ['baseline','control','candidate']];pairs.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,previous_ns=med[i,'previous',case],gain_vs_previous_percent=100*(1-c/med[i,'previous',case]),gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
  summary[case][label]=dict(gain_percent=st.median(r['gain_percent']for r in pairs),aa_absolute_percent=st.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
(p/'summary.json').write_text(json.dumps(dict(diagnostic_only=True,processes=48,cases=132,measured_observations=observations,warmup_observations=12672,layouts=layouts,results=summary,full_objective_achieved=False),indent=2)+'\n')
for op in ['count','empty','toggle','and','and_count','or','or_count','and_reverse','and_count_reverse','or_reverse','or_count_reverse']:
 subset=[r for case,s in summary.items()if case.split('/')[-1]==op for r in s.values()];print(op,'median gain',round(st.median(r['gain_percent']for r in subset),3),'range',round(min(r['gain_percent']for r in subset),3),round(max(r['gain_percent']for r in subset),3))
print('layout',layouts)
