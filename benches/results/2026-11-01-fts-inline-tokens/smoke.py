from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;root=p/'qualification-smoke';root.mkdir(exist_ok=False);status=[]
cases=['compare/tx_rollback_1row_1k/lin','compare/tx_rollback_1row_10k/lin','compare/update_1row_1k/lin','compare/delete_1row_1k/lin','compare/tx_commit_1row_1k/lin']
for shape in ['short','long']:
 m=json.loads((p/('provenance.json'if shape=='short'else'provenance-long.json')).read_text());assert m['source_restored']
 for role,b in m['binaries'].items():
  assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
  for case in cases:
   name=shape+'-'+role+'-'+case.replace('/','_');out=root/name;cmd=[b['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
   with(root/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1'),stdout=log,stderr=subprocess.STDOUT)
   status.append(dict(shape=shape,role=role,case=case,exit_code=r.returncode,command=cmd));(p/'smoke-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0,name
   data=json.loads((out/'run.json').read_text());assert data['status']=='complete' and len(data['cases'])==1 and data['cases'][0]['id']==case and len(data['observations'])==1
assert len(status)==20
print('20 short/long ordinary-engine functional smokes passed')
