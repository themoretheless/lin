from pathlib import Path
import json,math,statistics as st,itertools,hashlib
p=Path(__file__).resolve().parent;records=json.loads((p/'process-status.json').read_text());m=json.loads((p/'provenance.json').read_text());cases=['compare/insert_bulk_1k/lin','compare/insert_bulk_10k/lin'];roles=['original','control','disabled','traced'];assert len(records)==48 and {(r['round'],r['role'],r['case'])for r in records}==set(itertools.product(range(1,7),roles,cases));med={};traces={};observations=0
names=['unused_parent','text_collect','refs','lowercase','feature_hash','normalize','vector_copy','scratch_reset','row_install']
for r in records:
 assert r['exit']==0 and math.isfinite(r['elapsed_seconds'])and r['elapsed_seconds']>0 and r['binary_sha256']==m['binaries']['original'if r['role']in ['original','control']else'instrumented']['sha256'];dest=p/f"{r['round']:02}-{r['role']}"/r['case'].replace('/','_');q=json.loads((dest/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==r['case']and len(q['observations'])==8
 assert q['cases'][0]['contract']['lifecycle']=='fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64'
 values=[]
 for o in q['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1;v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);observations+=1
 med[r['round'],r['role'],r['case']]=st.median(values);f=dest.with_suffix('.trace.json')
 if r['role']=='traced':
  data=f.read_bytes();assert hashlib.sha256(data).hexdigest()==r['trace_sha256'];t=json.loads(data);assert t['phase_names']==names and t['timing_qualified']is False and 8<=len(t['records'])<=128
  for event in t['records']:
   assert isinstance(event['wall_ns'],int)and event['wall_ns']>0 and event['phase_calls']==[0,1,1]+[1000 if r['case']==cases[0]else 10000]*5+[1] and len(event['phase_ns'])==9 and event['phase_ns'][0]==0 and all(isinstance(v,int)and v>=0 for v in event['phase_ns'])and sum(event['phase_ns'])<=event['wall_ns']
  traces[r['round'],r['case']]=t['records']
 else:assert r['trace_sha256']is None and not f.exists()
assert observations==384
summary={}
for case in cases:
 aa=[];disabled=[];traced=[];process_fractions=[];residual=[];counts=0
 for round in range(1,7):
  o,c,d,t=[med[round,role,case]for role in roles];aa.append(abs(100*(1-c/o)));disabled.append(d/o);traced.append(t/d);events=traces[round,case];counts+=len(events);process_fractions.append([st.median(100*e['phase_ns'][i]/e['wall_ns']for e in events)for i in range(9)]);residual.append(st.median(100*(1-sum(e['phase_ns'])/e['wall_ns'])for e in events))
 summary[case]=dict(aa_absolute_percent=st.median(aa),disabled_over_original=st.median(disabled),traced_over_disabled=st.median(traced),trace_invocations=counts,phase_percent={name:st.median(v[i]for v in process_fractions)for i,name in enumerate(names)},unattributed_percent=st.median(residual))
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(dict(diagnostic_complete=True,processes=48,wall_observations=384,observer_calibration_recorded=True,performance_qualified=False,full_objective_achieved=False),indent=2)+'\n');lines=['# Bulk embedding phase diagnostic','','All48fresh processes/384wall observations retained. Original/control share one binary;disabled/traced share an instrumented binary. Phase shares are medians within each process then across processes;validation/calibration invocations are included andnot aligned1:1withmeasured observations. Clock/trace overhead is recorded,not hidden. No latency,source acceptance,RSS orpeer victory is qualified.','','|Case|A/A %|Disabled/original|Traced/disabled|Trace invocations|','|---|---:|---:|---:|---:|']
for case,r in summary.items():lines.append(f"|{case}|{r['aa_absolute_percent']:.4f}|{r['disabled_over_original']:.4f}|{r['traced_over_disabled']:.4f}|{r['trace_invocations']}|")
lines+=['','|Case|Phase|Caller wall %|','|---|---|---:|']
for case,r in summary.items():
 for name,value in r['phase_percent'].items():
  if name!='unused_parent':lines.append(f'|{case}|{name}|{value:.4f}|')
 lines.append(f"|{case}|unattributed|{r['unattributed_percent']:.4f}|")
lines+=['','Independent medians need not sumto100%. Only embedding subphases are traced;unattributed includes the rest of bulk INSERT and observer overhead. Input Db teardown andsetup are excluded. payload_bytes is unused by this CPU-only instrument andmust not be interpreted as a WAL measurement.']
(p/'report.md').write_text('\n'.join(lines)+'\n');print('complete48-process observer-calibrated phase matrix; no performance qualification')
