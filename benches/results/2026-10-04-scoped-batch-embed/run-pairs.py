import os, subprocess, pathlib, hashlib, json
root=pathlib.Path(__file__).resolve().parent
bins={k:pathlib.Path('/tmp/lin-scoped-embed-'+k) for k in ['baseline','candidate']}
(root/'binary-sha256.json').write_text(json.dumps({k:hashlib.sha256(p.read_bytes()).hexdigest() for k,p in bins.items()},indent=2))
for mode in ['native','durable']:
 for pair in range(1,7):
  for variant in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
   name=f'{mode}-{variant}-{pair}'
   env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1')
   env.pop('LIN_BENCH_MIXED_CASE',None)

   cmd=[str(bins[variant]),'--filter',('compare/insert_bulk_*/*' if mode=='native' else 'compare/durable_insert_*/*'),'--glob','--exclude','*/duckdb','--samples','24','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(root/name)]
   with (root/(name+'.log')).open('w') as log:subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
   print(name,flush=True)
