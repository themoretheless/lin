from pathlib import Path
import json,math,statistics as st,hashlib
p=Path(__file__).resolve().parent
cases=['compare/insert_bulk_1k/lin', 'compare/insert_bulk_10k/lin', 'compare/filter_eq/lin', 'compare/update_1row_1k/lin', 'compare/update_1row_100k/lin', 'compare/wal_scalar_full_update/lin', 'compare/wal_scalar_normal_update/lin', 'compare/insert_bulk_10k/sqlite']
records=json.loads((p/'process-status.json').read_text());provenance=json.loads((p/'provenance.json').read_text())
assert len(records)==288 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,m,c)for i in range(1,13)for m in ['baseline','control','candidate']for c in cases}
med={};observations=0
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
 d=json.loads((p/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json').read_text())
 assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case'] and len(d['observations'])==8
 values=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);observations+=1
 med[r['round'],r['mode'],r['case']]=st.median(values)
summary={};passed=True
for case in cases:
 summary[case]={};target=case.startswith('compare/insert_bulk_')and case.endswith('/lin')
 for series,start in [('series1',1),('series2',7)]:
  pairs=[]
  for i in range(start,start+6):
   a,b,c=[med[i,m,case]for m in ['baseline','control','candidate']]
   pairs.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
  gain=st.median(x['gain_percent']for x in pairs);noise=st.median(abs(x['aa_percent'])for x in pairs);positive=sum(x['gain_percent']>0 for x in pairs)
  ok=(positive==6 and gain>noise)if target else gain>=-max(2,noise);passed &=ok
  summary[case][series]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
assert observations==2304
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(p/'qualification.json').write_text(json.dumps(dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=288,observations=observations),indent=2)+'\n')
lines=['# Bulk owned-map template component comparison','','Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus owned-map bulk template candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.','','|Case|Series|Gain %|Absolute A/A %|Positive|Pass|','|---|---|---:|---:|---:|---|']
for case,series_rows in summary.items():
 for series,r in series_rows.items():lines.append(f"|{case}|{series}|{r['gain_percent']:.3f}|{r['aa_absolute_percent']:.3f}|{r['positive_pairs']}/6|{r['passes']}|")
lines+=['',f'Component gate: {passed}. All 288 processes / 2304 wall observations retained.']
(p/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(component_gate_passes=passed,processes=288,observations=observations)))
