from pathlib import Path
import subprocess,json,os,statistics
p=Path(__file__).resolve().parent
status={}
for pair in range(1,7):
 for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
  name=f'durable-1k-recheck-{v}-{pair}'
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1');env.pop('LIN_BENCH_MIXED_CASE',None)
  cmd=['/tmp/lin-bulk-hash-compare-'+v,'--filter','compare/durable_insert_1k/*','--glob','--exclude','*/duckdb','--samples','48','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  with (p/(name+'.log')).open('w') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  status[name]=r.returncode;(p/'durable-1k-recheck-status.json').write_text(json.dumps(status));print(name,r.returncode,flush=True)
  if r.returncode:raise SystemExit(r.returncode)
out={}
for e in ['lin','sqlite']:
 vals={}
 for v in ['baseline','candidate']:
  vals[v]=[]
  for i in range(1,7):
   d=json.loads((p/f'durable-1k-recheck-{v}-{i}/run.json').read_text());assert d['status']=='complete'
   obs=[x for x in d['observations'] if x['case']=='compare/durable_insert_1k/'+e];assert len(obs)==48
   vals[v].append(statistics.median(float(x['value'])/x['operations'] for x in obs))
 a,b=vals.values();out[e]={**vals,'paired_gain_percent':statistics.median((x-y)/x*100 for x,y in zip(a,b)),'wins':sum(y<x for x,y in zip(a,b))}
(p/'durable-1k-recheck.json').write_text(json.dumps(out,indent=2));print(json.dumps(out),flush=True)
