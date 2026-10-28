from pathlib import Path
import json,subprocess,os,hashlib,ast
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());cases=ast.literal_eval(next(x[6:]for x in(p/'run.py').read_text().splitlines()if x.startswith('cases=')))
dest=p/'smoke';dest.mkdir(exist_ok=False);records=[]
for role in ['baseline','candidate']:
 binary=m['binaries'][role];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
 for case in cases:
  out=dest/(role+'-'+case.replace('/','_'));cmd=[binary['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
  with out.with_suffix('.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C'))
  records.append(dict(role=role,case=case,exit=r.returncode));(p/'smoke-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0
  q=json.loads((out/'run.json').read_text());assert q['status']=='complete'and len(q['observations'])==1 and q['cases'][0]['id']==case
assert len(records)==18
