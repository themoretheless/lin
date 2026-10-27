from pathlib import Path
import subprocess,json,hashlib,os,time
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status=[]
CASES={f'compare/{kind}_{size}/{engine}'for kind in ['insert_bulk','durable_insert']for size in ['1k','10k']for engine in ['lin','sqlite']}
for mode in ['baseline','candidate']:assert hashlib.sha256(Path(f'/tmp/lin-reused-embed-integration-{mode}').read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
for i in range(1,13):
 modes=['baseline','control','candidate'];shift=i%3;modes=modes[shift:]+modes[:shift]
 for mode in modes:
  name=f'{i:02}-{mode}';binary=f'/tmp/lin-reused-embed-integration-{"candidate"if mode=="candidate"else"baseline"}';dest=p/name
  cmd=[binary,'--filter','compare/*insert*/*','--glob','--exclude','*insert_native*','--exclude','*insert_phase*','--exclude','*/duckdb','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');load=os.getloadavg();started=time.time()
  with(p/f'{name}.log').open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(run=name,exit_code=r.returncode,load_before=load,load_after=os.getloadavg(),elapsed_seconds=time.time()-started));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
  data=json.loads((dest/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==8 and len(data['observations'])==64
  assert {o['case']for o in data['observations']}==CASES and all(o['operations']==1 for o in data['observations'])
  print(name,flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
