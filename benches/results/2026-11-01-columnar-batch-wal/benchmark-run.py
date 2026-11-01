from pathlib import Path
import json,hashlib,subprocess,platform,os,sys,time
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule
p=Path(__file__).resolve().parent;smoke='--smoke'in sys.argv
assert smoke or(platform.system()=='Linux'and platform.machine()=='x86_64')
m=json.loads((p/'benchmark-provenance.json').read_text());assert m['source_restored'];cases=json.loads((p/'benchmark-cases.json').read_text());assert len(cases)==24
if not smoke:
 frozen=json.loads((p/'frozen-benchmark.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in frozen.items())
 assert json.loads((p/'functional/status.json').read_text())['complete']and json.loads((p/'legacy-oracle-negative/status.json').read_text())['runtime_negative_caught']and json.loads((p/'benchmark-smoke/status.json').read_text())['complete']and json.loads((p/'legacy-results/status.json').read_text())['complete']and json.loads((p/'mutant-status.json').read_text())['runtime_assertion_caught']
planned=[dict(round=1,workload=c['id'],role=role)for c in cases[1:]+cases[:1]for role in ['baseline','candidate']]if smoke else schedule(cases,['baseline','control','candidate'])
root=p/('benchmark-smoke'if smoke else'benchmark-measurement');root.mkdir(exist_ok=False);rows=[];env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None);assert not env.get('LD_PRELOAD')
(root/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),timing_qualification=not smoke,balanced_measurement_plan=not smoke),indent=2)+'\n')
for item in planned:
 repeat=item['round'];role=item['role'];case=item['workload'];b=m['binaries']['baseline'if role=='control'else role];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256'];name=f'{repeat:02}-{role}-'+case.replace('/','_');out=root/name;cmd=[b['path'],'--filter',case,'--samples','1'if smoke else'8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)];start=time.time()
 with(root/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 rows.append(dict(round=repeat,role=role,id=case,command=cmd,exit_code=r.returncode,binary_sha256=b['sha256'],output=name,elapsed_seconds=time.time()-start));(root/'process-status.json').write_text(json.dumps(rows,indent=2)+'\n');assert r.returncode==0,rows[-1]
 d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==(1 if smoke else 8)
 print(repeat,case,role,'complete',flush=True)
assert len(rows)==(48 if smoke else 864)
(root/'status.json').write_text(json.dumps(dict(complete=True,processes=len(rows),smoke=smoke),indent=2)+'\n')
