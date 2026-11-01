from pathlib import Path
import json,hashlib,subprocess,os
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']and m['single_binary_for_all_modes'];assert hashlib.sha256(Path(m['binary']).read_bytes()).hexdigest()==m['binary_sha256'];root=p/'smoke';root.mkdir(exist_ok=False);rows=[];env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None)
for size in ['1k','10k']:
 for op in ['update','delete','commit','rollback']:
  for mode in ['chunked','grouped']:
   for engine in ['lin','sqlite_wal']:
    name=f'{size}-{op}-{mode}-{engine}';case=f'compare/seed_history_{mode}_{op}_{size}/{engine}';out=root/name;cmd=[m['binary'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
    with(root/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
    rows.append(dict(case=case,command=cmd,exit_code=r.returncode,output=name));(root/'status.json').write_text(json.dumps(dict(complete=False,cases=rows,performance_qualification=False),indent=2)+'\n');assert r.returncode==0,name
    d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==1
    print(name,'full values verified',flush=True)
assert len(rows)==32
(root/'status.json').write_text(json.dumps(dict(complete=True,cases=rows,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
