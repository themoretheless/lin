from pathlib import Path
import subprocess,os,json,hashlib,sys
p=Path(__file__).resolve().parent
binary=Path('/tmp/lin-insert-profile-after-sharing')
(p/'binary-sha256.json').write_text(json.dumps({'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
status=[]
for i in range(1,4):
 for mode in (['native','durable'] if i%2 else ['durable','native']):
  name=f'{mode}-{i}'
  cmd=[str(binary),'--filter',('compare/insert_bulk_*/lin' if mode=='native' else 'compare/durable_insert_*/lin'),'--glob','--samples','24','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1');env.pop('LIN_BENCH_MIXED_CASE',None)
  with (p/(name+'.log')).open('w') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  status.append({'run':name,'exit_code':r.returncode});(p/'runner-status.json').write_text(json.dumps(status,indent=2))
  print(name,r.returncode,flush=True)
  if r.returncode:sys.exit(r.returncode)
