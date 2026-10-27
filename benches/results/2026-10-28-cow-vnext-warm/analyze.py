from pathlib import Path
import json,hashlib,subprocess,math,statistics
p=Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
h=lambda b:hashlib.sha256(b).hexdigest()
m=load('provenance.json');status=load('status.json');assert m['source_restored']and not m['performance_qualified']
for f,digest in m['source_sha256'].items():assert h(subprocess.check_output(['git','show',m['source_commit']+':'+f]))==digest,f
assert h((p/'gate.rs').read_bytes())==m['gate_sha256']
tests=load('test-status.json');assert tests['exit_code']==0 and tests['source_restored'] and tests['source_sha256']==m['source_sha256']
import re
assert tests['passed']==443 and tests['ignored']==2
for role,binary in m['binaries'].items():
 for f,digest in binary['candidate_sources_sha256'].items():
  expected=h(subprocess.check_output(['git','show',m['baseline_commit']+':'+f])) if role=='baseline' else m['source_sha256'][f]
  assert expected==digest
assert len(status)==36 and len({r['pid']for r in status})==36 and {(r['repeat'],r['mode'])for r in status}=={(i,mode)for i in range(1,13)for mode in ['baseline','control','candidate']}
med={};observations=0
for run in status:
 assert run['exit_code']==0 and run['binary_sha256']==m['binaries']['candidate'if run['mode']=='candidate'else'baseline']['sha256']
 rows=[json.loads(line)for line in(p/f"{run['repeat']:02}-{run['mode']}.jsonl").read_text().splitlines()];assert len(rows)==96
 for r in rows:
  assert r['exact_readback']is True and r['rows']in [1000,10000,100000]and r['sample']in range(16)
  metric='rollback_ns'if'rollback_ns'in r else'ns_per_update';assert math.isfinite(r[metric])and r[metric]>0 and r['operations']==(1 if metric=='rollback_ns'else 512);observations+=1
 for size in [1000,10000,100000]:
  for metric in ['rollback_ns','ns_per_update']:
   selected=[r for r in rows if r['rows']==size and metric in r];assert len(selected)==16 and {r['sample']for r in selected}==set(range(16));med[run['repeat'],run['mode'],size,metric]=statistics.median(r[metric]for r in selected)
assert observations==3456
results=[];accepted=True
for size in [1000,10000,100000]:
 for metric in ['rollback_ns','ns_per_update']:
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    a,b,c=[med[i,mode,size,metric]for mode in ['baseline','control','candidate']];pairs.append(dict(repeat=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
   gain=statistics.median(r['gain_percent']for r in pairs);aa=statistics.median(abs(r['aa_percent'])for r in pairs);positive=sum(r['gain_percent']>0 for r in pairs);target=True;passed=(positive==6 and gain>aa)if target else gain>=-max(2,aa);accepted &=passed
   results.append(dict(rows=size,metric=metric,series=label,target=target,gain_percent=gain,aa_absolute_percent=aa,positive_pairs=positive,passes=passed,pairs=pairs))
q=dict(diagnostic_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,processes=36,observations=observations,results=results);(p/'summary.json').write_text(json.dumps(q,indent=2)+'\n')
for r in results:print(r['rows'],r['metric'],r['series'],round(r['gain_percent'],3),round(r['aa_absolute_percent'],3),r['positive_pairs'],r['passes'])
print('Independent raw/source audit PASS; diagnostic gate:',accepted)
