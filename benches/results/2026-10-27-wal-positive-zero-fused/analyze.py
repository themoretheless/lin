from pathlib import Path
import json,statistics as st
p=Path(__file__).resolve().parent;rows=json.loads((p/'cpu-records.json').read_text());assert len(rows)==432;cases=sorted({r['case']for r in rows});assert len(cases)==12;index={(r['repeat'],r['case'],r['role']):r for r in rows};assert len(index)==432;results=[];accepted=True
for case in cases:
 for series,start in [('series1',1),('series2',7)]:
  gain=[];aa=[]
  for i in range(start,start+6):
   a,b,c=[index[i,case,m]['ns_per_op']for m in ['baseline','control','candidate']];gain.append(100*(1-c/a));aa.append(abs(100*(1-b/a)))
  g,n=st.median(gain),st.median(aa);target=case in ['doc_update','doc_delete'];passed=(all(v>0 for v in gain)and g>n)if target else g>=-max(2,n);accepted &=passed
  results.append(dict(case=case,series=series,target=target,gain_percent=g,aa_percent=n,positive_pairs=sum(v>0 for v in gain),passes=passed))
(p/'cpu-summary.json').write_text(json.dumps(dict(cpu_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,results=results),indent=2)+'\n')
print('CPU gate:',accepted)
