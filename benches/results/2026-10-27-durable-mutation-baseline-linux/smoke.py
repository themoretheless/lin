from pathlib import Path
import hashlib,json,os,subprocess
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());b=m['binary'];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1');env.pop('LIN_BENCH_MIXED_CASE',None)
status=[]
for size in ['1k','10k']:
 for op in ['update','delete','commit','rollback']:
  for peer in ['lin','sqlite_delete','sqlite_wal']:
   case=f'compare/durable_{op}_1row_{size}/{peer}';dest=p/'smoke'/case.replace('/','_');dest.parent.mkdir(exist_ok=True)
   with dest.with_suffix('.log').open('w')as log:r=subprocess.run([b['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)],env=env,stdout=log,stderr=subprocess.STDOUT)
   assert r.returncode==0,(case,'inspect log')
   d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and len(d['observations'])==1 and d['observations'][0]['operations']==1
   status.append(dict(case=case,exit_code=0));(p/'smoke-status.json').write_text(json.dumps(dict(correctness_only=True,performance_qualified=False,runs=status),indent=2)+'\n');print(case,'PASS',flush=True)
assert len(status)==24
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
