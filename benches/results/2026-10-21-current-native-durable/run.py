from pathlib import Path
import os,subprocess,json,hashlib,sys
root=Path(__file__).resolve().parent
binary=Path('/tmp/lin-native-durable-20261021')
(root/'binary-sha256.json').write_text(json.dumps({'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
completed=[]
for i in range(1,7):
 for mode in (['native','durable'] if i%2 else ['durable','native']):
  name=f'{mode}-{i}'
  cmd=[str(binary),'--filter',('compare/insert_bulk_*/*' if mode=='native' else 'compare/durable_insert_*/*'),'--glob','--exclude','*/duckdb','--samples','24','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(root/name)]
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1');env.pop('LIN_BENCH_MIXED_CASE',None)
  with (root/(name+'.log')).open('w') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  completed.append({'run':name,'exit_code':r.returncode})
  (root/'runner-status.json').write_text(json.dumps(completed,indent=2))
  print(name,r.returncode,flush=True)
  if r.returncode:sys.exit(r.returncode)
