from pathlib import Path
import json,hashlib,subprocess,platform,os,sys,time
p=Path(__file__).resolve().parent;smoke='--smoke'in sys.argv
assert smoke or(platform.system()=='Linux'and platform.machine()=='x86_64')
m=json.loads((p/'provenance.json').read_text());assert m['source_restored'];cases=json.loads((p/'cases.json').read_text());assert len(cases)==20
if not smoke:
 assert json.loads((p/'smoke/status.json').read_text())['complete']and json.loads((p/'barrier-check/status.json').read_text())['compiled_negative_caught']
root=p/('smoke'if smoke else'measurement');root.mkdir(exist_ok=False);rows=[];env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None);assert not env.get('LD_PRELOAD')
(root/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),timing_qualification=not smoke),indent=2)+'\n')
for repeat in range(1,2 if smoke else 13):
 for i,c in enumerate(cases[repeat%20:]+cases[:repeat%20]):
  roles=['baseline','candidate']if smoke else['baseline','control','candidate'];shift=(repeat+i)%len(roles);roles=roles[shift:]+roles[:shift]
  for role in roles:
   b=m['binaries']['baseline'if role=='control'else role];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
   name=f'{repeat:02}-{role}-'+c['id'].replace('/','_');out=root/name;cmd=[b['path'],'--filter',c['id'],'--samples','1'if smoke else'8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
   start=time.time()
   with(root/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
   rows.append(dict(round=repeat,role=role,id=c['id'],command=cmd,exit_code=r.returncode,binary_sha256=b['sha256'],output=name,elapsed_seconds=time.time()-start));(root/'process-status.json').write_text(json.dumps(rows,indent=2)+'\n');assert r.returncode==0,rows[-1]
   d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==c['id']and len(d['observations'])==(1 if smoke else 8)
  print(repeat,c['id'],'complete',flush=True)
assert len(rows)==(40 if smoke else 720)
(root/'status.json').write_text(json.dumps(dict(complete=True,processes=len(rows),smoke=smoke),indent=2)+'\n')
