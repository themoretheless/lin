from pathlib import Path
import json,hashlib,re,math,runpy
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda f:json.loads((p/f).read_text())
frozen=read('frozen.json');assert all(sha(Path(f).read_bytes())==h for f,h in frozen.items())
log=(p/'full-release.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log);assert sum(int(x[0])for x in counts)==458 and sum(int(x[2])for x in counts)==2
m=read('provenance.json');assert m['source_restored']and m['candidate_persist_sha256']==sha(Path('src/persist.rs').read_bytes())and m['harness_sha256']==sha((p/'compare.rs').read_bytes())
assert all(sha(Path(f).read_bytes())==h for f,h in m['other_source_sha256'].items())
for b in m['binaries'].values():assert sha(Path(b['path']).read_bytes())==b['sha256']
barrier=read('barrier-check/status.json');assert barrier['complete']and barrier['compiled_negative_caught'];scope=runpy.run_path(str(p.parent/'2026-11-01-wal-syscall-diagnostic/audit-traces.py'))['scope']
for role,expected in [('baseline',['fdatasync']),('candidate',['fsync']),('missing_barrier',[])]:
 r=barrier['results'][role];f=p/'barrier-check'/(role+'.strace');assert r['exit_code']==0 and sha(f.read_bytes())==r['trace_sha256'];actual=[c['call']for c in scope(f.read_text())if c['call']in ['fsync','fdatasync']];assert actual==expected and r['missing_barrier_rejected']==(role=='missing_barrier')
assert read('smoke/status.json')['complete'];cases=read('cases.json');rows=read('measurement/process-status.json');expected=[]
for rnd in range(1,13):
 for i,c in enumerate(cases[rnd%20:]+cases[:rnd%20]):
  roles=['baseline','control','candidate'];s=(rnd+i)%3
  expected.extend((rnd,role,c['id'])for role in roles[s:]+roles[:s])
assert [(r['round'],r['role'],r['id'])for r in rows]==expected and len(rows)==720
medians={};ids=set()
def median(values):
 v=sorted(values);n=len(v);return v[n//2]if n%2 else(v[n//2-1]+v[n//2])/2
for folder,count in [('smoke',40),('measurement',720)]:
 rs=read(folder+'/process-status.json');assert len(rs)==count
 for r in rs:
  b=m['binaries']['baseline'if r['role']=='control'else r['role']];out=p/folder/r['output'];samples='1'if folder=='smoke'else'8'
  assert r['exit_code']==0 and r['binary_sha256']==b['sha256']and r['command']==[b['path'],'--filter',r['id'],'--samples',samples,'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
  d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['id'] and len(d['observations'])==int(samples)
  assert d['id']not in ids;ids.add(d['id']);values=[]
  for o in d['observations']:
   v=float(o['value']);assert o['case']==r['id']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'and math.isfinite(v)and v>0;values.append(v)
  if folder=='measurement':medians[r['round'],r['role'],r['id']]=median(values)
summary=read('measurement/summary.json');passed=True
for c in cases:
 for series,start in [('series1',1),('series2',7)]:
  gains=[];aa=[]
  for rnd in range(start,start+6):
   a,b,v=[medians[rnd,role,c['id']]for role in ['baseline','control','candidate']];gains.append(100*(1-v/a));aa.append(abs(100*(1-b/a)))
  g=median(gains);noise=median(aa);positive=sum(v>0 for v in gains);ok=(positive==6 and g>noise)if c['target']else g>=-max(2,noise);r=summary[c['id']][series]
  assert abs(r['gain_percent']-g)<1e-10 and abs(r['aa_absolute_percent']-noise)<1e-10 and r['positive_pairs']==positive and r['passes']==ok;passed &=ok
q=read('measurement/qualification.json');assert q['processes']==720 and q['observations']==5760 and q['component_gate_passes']==passed and not q['full_objective_achieved']
(p/'independent-audit.json').write_text(json.dumps(dict(complete=True,processes=720,observations=5760,component_gate_passes=passed,compiled_missing_barrier_caught=True,full_objective_achieved=False),indent=2)+'\n')
print('PASS frozen source, actual barriers, 40 smokes, 720 commands / 5760 observations and numerical gates')
