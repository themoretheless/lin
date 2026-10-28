from pathlib import Path
import json,subprocess,os,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
cases=['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin','compare/update_1row_1k/lin','compare/update_1row_100k/lin'];status=[]
for role in ['baseline','candidate']:
 binary=m['binaries'][role];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
 for case in cases:
  dest=p/'smoke'/role/case.replace('/','_');dest.parent.mkdir(parents=True,exist_ok=True)
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None)
  with dest.with_suffix('.log').open('w')as log:r=subprocess.run([binary['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)],env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,(role,case)
  d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==1 and d['observations'][0]['case']==case and d['observations'][0]['operations']==1
  status.append(dict(role=role,case=case,exit_code=0));(p/'smoke-status.json').write_text(json.dumps(dict(correctness_only=True,performance_qualified=False,runs=status),indent=2)+'\n')
assert len(status)==8 and all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==d for f,d in m['source_sha256'].items())
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
print('Eight fresh BTreeMap correctness/reopen smokes passed')
