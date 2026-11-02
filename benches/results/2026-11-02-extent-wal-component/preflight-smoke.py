from pathlib import Path
import hashlib,json,os,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
m=json.loads((p/'preflight-build.json').read_text());assert m['exit_code']==0 and m['source_unchanged'];binary=m['binary'];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
folder=p/'local-smokes';folder.mkdir(exist_ok=False);results=[]
for mode in ['legacy','extent']:
 for op,n in [('update','1k'),('commit','1k'),('rollback','1k'),('delete','10k')]:
  case=f'compare/durable_{op}_1row_{n}/lin';dest=folder/f'{mode}-{op}-{n}';command=[binary['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  env=dict(os.environ,LIN_BENCH_WAL_LAYOUT=mode,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1')
  with dest.with_suffix('.log').open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,(mode,case,r.returncode)
  d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==1 and d['observations'][0]['case']==case
  results.append(dict(layout=mode,case=case,exit_code=0,binary_sha256=binary['sha256'],full_reopen_oracle_passed=True))
(p/'preflight-smoke.json').write_text(json.dumps(dict(local_only=True,performance_qualified=False,smokes=len(results),checks=results),indent=2)+'\n');print('PASS eight actual legacy/extent Full-mode reopen smoke cases; no timing qualification')
