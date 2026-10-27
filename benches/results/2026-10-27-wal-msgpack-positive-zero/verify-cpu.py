from pathlib import Path
import json,hashlib,subprocess,math
p=Path(__file__).resolve().parent;root=p.parents[2]
load=lambda n:json.loads((p/n).read_text())
def median(values):
 v=sorted(values);return(v[(len(v)-1)//2]+v[len(v)//2])/2
meta=load('cpu-provenance.json');base=load('test-status.json');rows=load('cpu-records.json');status=load('cpu-status.json');q=load('cpu-summary.json')
assert base['source_restored']and meta['source_sha256']==base['source_sha256']
for name,digest in meta['source_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',base['source_commit']+':'+name],cwd=root)).hexdigest()==digest,name
for name,key in [('helper.rs','helper_sha256'),('oracle.rs','oracle_sha256')]:assert hashlib.sha256((p/name).read_bytes()).hexdigest()==meta[key]
assert len(status)==12 and {r['repeat']for r in status}==set(range(1,13))and len({r['pid']for r in status})==12 and all(r['exit_code']==0 for r in status)
cases={r['case']for r in rows};expected={'doc_update','doc_delete','dense_update','negative_zero_update','zero_update','empty_update','no_vector_update','unrelated_insert','batch'}
assert cases in [expected,expected|{'prefix_only_zeros','late_zeros_update','mixed_vectors'}]
assert len(rows)==12*3*len(cases);index={(r['repeat'],r['case'],r['role']):r for r in rows};assert set(index)=={(i,c,m)for i in range(1,13)for c in cases for m in ['baseline','control','candidate']}
for r in rows:assert r['operations']==4000 and math.isfinite(r['ns_per_op'])and r['ns_per_op']>0 and type(r['payload_bytes'])is int and r['payload_bytes']>0
for c in cases:
 for i in range(1,13):assert index[i,c,'baseline']['payload_bytes']==index[i,c,'control']['payload_bytes']and index[i,c,'candidate']['payload_bytes']<=index[i,c,'baseline']['payload_bytes']
results=[];accepted=True
for c in sorted(cases):
 for label,start in [('series1',1),('series2',7)]:
  gains=[];noise=[]
  for i in range(start,start+6):
   a=index[i,c,'baseline']['ns_per_op'];b=index[i,c,'control']['ns_per_op'];d=index[i,c,'candidate']['ns_per_op'];gains.append(100*(1-d/a));noise.append(abs(100*(1-b/a)))
  gain,aa=median(gains),median(noise);target=c in {'doc_update','doc_delete'};positive=sum(g>0 for g in gains);passed=(positive==6 and gain>aa)if target else gain>=-max(2,aa);accepted &=passed
  results.append(dict(case=c,series=label,target=target,gain_percent=gain,aa_percent=aa,positive_pairs=positive,passes=passed))
assert q==dict(cpu_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,results=results)
print('Independent CPU audit PASS:',len(rows),'blocks;',len(rows)*4000,'encodes; gate:',accepted,';no full-engine acceptance')
