from pathlib import Path
import sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
import json,math,statistics as st,hashlib,sys
p=Path(sys.argv[1]).resolve()if len(sys.argv)>1 else Path(__file__).resolve().parent;root=p/'benchmark-measurement';cases=json.loads((p/'benchmark-cases.json').read_text());rows=json.loads((root/'process-status.json').read_text());assert len(cases)==24 and len(rows)==864
plan=schedule(cases,['baseline','control','candidate']);expected=[(x['round'],x['role'],x['workload'])for x in plan]
assert [(r['round'],r['role'],r['id'])for r in rows]==expected
orders_from_records(rows,[c['id']for c in cases],['baseline','control','candidate'],'id')
provenance=json.loads((p/'benchmark-provenance.json').read_text())
med={};run_ids=set();total=0
for r in rows:
 c=next(c for c in cases if c['id']==r['id']);assert r['exit_code']==0
 b=provenance['binaries']['baseline'if r['role']=='control'else r['role']];assert r['binary_sha256']==b['sha256']
 d=json.loads((root/r['output']/'run.json').read_text());assert d['status']=='complete' and len(d['cases'])==1 and d['cases'][0]['id']==c['id']and len(d['observations'])==8
 assert isinstance(d['id'],str) and d['id'] not in run_ids;run_ids.add(d['id'])
 values=[]
 for o in d['observations']:
  assert o['case']==c['id']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available';v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);total+=1
 med[r['round'],r['role'],c['id']]=st.median(values)
assert total==6912
summary={};passed=True
for c in cases:
 result={}
 for series,start in [('series1',1),('series2',7)]:
  pairs=[]
  for i in range(start,start+6):
   a,b,v=[med[i,role,c['id']]for role in ['baseline','control','candidate']];pairs.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=v,gain_percent=100*(1-v/a),aa_percent=100*(1-b/a)))
  gain=st.median(x['gain_percent']for x in pairs);noise=st.median(abs(x['aa_percent'])for x in pairs);positive=sum(x['gain_percent']>0 for x in pairs);ok=(positive==6 and gain>noise)if c['target']else gain>=-max(2,noise);passed &=ok
  result[series]=dict(target=c['target'],gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
 summary[c['id']]=result
(root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(root/'qualification.json').write_text(json.dumps(dict(measurement_complete=True,processes=864,observations=total,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False),indent=2)+'\n')
lines=['# Compatible column Batch WAL component benchmark','','864 processes / 6912 observations. Diagnostic component; whole-engine and eight-peer objective remain unqualified.','','|Case|Series|Gain %|A/A %|Positive|Pass|','|---|---|---:|---:|---:|---|']
for case,series in summary.items():
 for name,r in series.items():lines.append(f"|{case}|{name}|{r['gain_percent']:.3f}|{r['aa_absolute_percent']:.3f}|{r['positive_pairs']}/6|{r['passes']}|")
(root/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(component_gate_passes=passed,observations=total)))
