from pathlib import Path
import sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule
import platform,subprocess,os,json,hashlib,time
assert platform.system()=='Linux'and platform.machine()=='x86_64'
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda f:json.loads((p/f).read_text());m=read('provenance.json');frozen=read('frozen.json');assert m['source_restored']and m['single_binary_for_all_modes']and sha(Path(m['binary']).read_bytes())==m['binary_sha256'];assert all(sha(Path(f).read_bytes())==h for f,h in frozen.items())
assert read('seed-equivalence/status.json')['complete']and read('smoke/status.json')['complete']and read('setup-traces/status.json')['complete']
cases=read('cases.json');roles=read('roles.json');assert len(cases)==8 and len(roles)==6;planned=schedule(cases,[x['id']for x in roles])
root=p/'measurement';root.mkdir(exist_ok=False);records=[];env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None);env.pop('LIN_COLUMN_BATCH_FIXTURE',None);assert not env.get('LD_PRELOAD')
(root/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualification=False,diagnostic_only=True),indent=2)+'\n')
for item in planned:
 rnd=item['round'];c=next(x for x in cases if x['id']==item['workload']);r=next(x for x in roles if x['id']==item['role'])
 assert sha(Path(m['binary']).read_bytes())==m['binary_sha256'];case=f"compare/seed_history_{r['mode']}_{c['operation']}_{c['size']}/{r['engine']}";name=f"{rnd:02}-{c['id']}-{r['id']}";out=root/name;cmd=[m['binary'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)];start=time.time()
 with(root/(name+'.log')).open('w')as log:result=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 records.append(dict(round=rnd,workload=c['id'],role=r['id'],case=case,command=cmd,exit_code=result.returncode,binary_sha256=m['binary_sha256'],output=name,elapsed_seconds=time.time()-start));(root/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert result.returncode==0,records[-1]
 d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==8
 print(rnd,c['id'],r['id'],'complete',flush=True)
assert len(records)==576
(root/'status.json').write_text(json.dumps(dict(complete=True,processes=576,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
