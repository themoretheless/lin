import os, subprocess, pathlib, hashlib, json
root=pathlib.Path(__file__).resolve().parent
bins={k:pathlib.Path('/tmp/lin-fts-union-'+k) for k in ['baseline','candidate']}
(root/'binary-sha256.json').write_text(json.dumps({k:hashlib.sha256(p.read_bytes()).hexdigest() for k,p in bins.items()},indent=2))
for pair in range(1,7):
 for variant in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
  name=f'{variant}-{pair}'
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1')
  cmd=[str(bins[variant]),'--filter','compare/fts_lex_*/lin','--glob','--samples','32','--max-iterations','100','--warmup-ms','50','--sample-ms','5','--output',str(root/name)]
  with (root/(name+'.log')).open('w') as log: subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  print(name,flush=True)
