from pathlib import Path
import subprocess,json,os,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary='/tmp/lin-mutation-phase-profile';assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==meta['binary_sha256'];status=[]
for i in range(1,4):
 for label in ['1k','10k','100k']:
  for kind in ['update','delete']:
   name=f'{kind}-{label}-{i}';cmd=[binary,'--filter',f'compare/{kind}_1row_{label}/lin','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
   env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None)
   with (p/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
   status.append(dict(run=name,exit_code=r.returncode,load_after=os.getloadavg()));(p/'status.json').write_text(json.dumps(status,indent=2));assert r.returncode==0;print(name,0,flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
