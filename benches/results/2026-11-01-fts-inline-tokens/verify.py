from pathlib import Path
import json,hashlib,math,re
p=Path(__file__).resolve().parent;root=p/'measurement';h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
for f,digest in json.loads((p/'frozen.json').read_text()).items():assert h(f)==digest,f
assert json.loads((p/'test-status.json').read_text())['passed']==459
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(p/'full-release.log').read_text())];assert [sum(x[i]for x in counts)for i in range(3)]==[459,0,2]
negative=json.loads((p/'mutant-status.json').read_text());assert negative['source_restored']and negative['runtime_assertion_caught']and negative['exit_code']==101
assert 'test result: FAILED.'in(p/'truncated-spill-runtime.log').read_text()
assert h('src/fts.rs')==h(p/'candidate-fts.rs')==negative['original_sha256']
cases=json.loads((p/'cases.json').read_text());rows=json.loads((root/'process-status.json').read_text());assert len(rows)==756
assert json.loads((p/'smoke-all/status.json').read_text())==dict(complete=True,processes=42,smoke=True)
proof={shape:json.loads((p/('provenance.json'if shape=='short'else'provenance-long.json')).read_text())for shape in ['short','long']}
for shape,m in proof.items():
 assert m['source_restored'] and m['candidate_fts_sha256']==h(p/'candidate-fts.rs')and m['baseline_fts_sha256']==h(p/'baseline-fts.rs')
 assert m['harness_sha256']==h(p/('compare.rs'if shape=='short'else'compare-long.rs'))
 for b in m['binaries'].values():assert h(b['path'])==b['sha256']
expected=[]
for repeat in range(1,13):
 for i,c in enumerate(cases[repeat%21:]+cases[:repeat%21]):
  roles=['baseline','control','candidate'];shift=(repeat+i)%3;roles=roles[shift:]+roles[:shift];expected.extend((repeat,role,c['id'])for role in roles)
assert [(r['round'],r['role'],r['id'])for r in rows]==expected
median=lambda v:(sorted(v)[(len(v)-1)//2]+sorted(v)[len(v)//2])/2
med={};ids=set()
for r in rows:
 c=next(c for c in cases if c['id']==r['id']);b=proof[c['shape']]['binaries']['baseline'if r['role']=='control'else r['role']]
 name=f"{r['round']:02}-{r['role']}-"+c['id'].replace('/','_').replace('|','_');assert r['output']==name and r['exit_code']==0 and r['binary_sha256']==b['sha256']
 assert r['command']==[b['path'],'--filter',c['case'],'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(root/name)]
 d=json.loads((root/name/'run.json').read_text());assert d['id']not in ids;ids.add(d['id']);assert d['status']=='complete'and len(d['observations'])==8 and len(d['cases'])==1 and d['cases'][0]['id']==c['case']
 v=[]
 for o in d['observations']:
  assert o['case']==c['case']and o['operations']==1 and o['metric']=='wall'and o['availability']['state']=='available';x=float(o['value']);assert math.isfinite(x)and x>0;v.append(x)
 med[r['round'],r['role'],c['id']]=median(v)
summary=json.loads((root/'summary.json').read_text());passed=True
for c in cases:
 for series,start in [('series1',1),('series2',7)]:
  gains=[];noise=[]
  for i in range(start,start+6):
   a,b,v=[med[i,role,c['id']]for role in ['baseline','control','candidate']];gains.append(100*(1-v/a));noise.append(abs(100*(1-b/a)))
  gain=median(gains);aa=median(noise);positive=sum(x>0 for x in gains);ok=(positive==6 and gain>aa)if c['target']else gain>=-max(2,aa);passed &=ok
  r=summary[c['id']][series];assert r['gain_percent']==gain and r['aa_absolute_percent']==aa and r['positive_pairs']==positive and r['passes']==ok
q=json.loads((root/'qualification.json').read_text());assert q==dict(measurement_complete=True,processes=756,observations=6048,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False)
(root/'independent-audit.json').write_text(json.dumps(dict(audit_complete=True,processes=756,observations=6048,component_gate_passes=passed,full_objective_achieved=False),indent=2)+'\n');print('Independent exact-command and raw numerical audit passed')
