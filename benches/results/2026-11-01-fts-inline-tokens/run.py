from pathlib import Path
import json,hashlib,subprocess,os,platform,time,sys
p=Path(__file__).resolve().parent;smoke='--smoke'in sys.argv
assert smoke or (platform.system()=='Linux' and platform.machine()=='x86_64')
cases=json.loads((p/'cases.json').read_text());assert len(cases)==21
root=p/('smoke-all'if smoke else'measurement');root.mkdir(exist_ok=False)
if not smoke:assert json.loads((p/'smoke-all/status.json').read_text())['complete']
provenance={shape:json.loads((p/('provenance.json'if shape=='short'else'provenance-long.json')).read_text())for shape in ['short','long']}
assert all(m['source_restored']for m in provenance.values())
env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None)
assert not env.get('LD_PRELOAD') and not env.get('DYLD_INSERT_LIBRARIES')
(root/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),timing_qualification=not smoke),indent=2)+'\n')
rows=[]
for repeat in range(1,2 if smoke else 13):
 ordered=cases[repeat%21:]+cases[:repeat%21]
 for i,c in enumerate(ordered):
  roles=['baseline','candidate']if smoke else['baseline','control','candidate'];shift=(repeat+i)%len(roles);roles=roles[shift:]+roles[:shift]
  for role in roles:
   b=provenance[c['shape']]['binaries']['baseline'if role=='control'else role];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
   name=f"{repeat:02}-{role}-"+c['id'].replace('/','_').replace('|','_');out=root/name;cmd=[b['path'],'--filter',c['case'],'--samples','1'if smoke else'8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
   start=time.time()
   with(root/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
   rows.append(dict(round=repeat,role=role,id=c['id'],shape=c['shape'],case=c['case'],command=cmd,exit_code=r.returncode,binary_sha256=b['sha256'],output=name,elapsed_seconds=time.time()-start));(root/'process-status.json').write_text(json.dumps(rows,indent=2)+'\n');assert r.returncode==0,rows[-1]
   d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==c['case'] and len(d['observations'])==(1 if smoke else 8)
  print(repeat,c['id'],'complete',flush=True)
assert len(rows)==(42 if smoke else 756)
(root/'status.json').write_text(json.dumps(dict(complete=True,processes=len(rows),smoke=smoke),indent=2)+'\n')
