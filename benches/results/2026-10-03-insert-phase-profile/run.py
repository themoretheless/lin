from pathlib import Path
import os,subprocess,json,hashlib
root=Path(__file__).resolve().parent
binary=Path('/tmp/lin-insert-phase-profile')
(root/'binary-sha256.json').write_text(json.dumps({'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
for i in range(1,4):
 name=f'process-{i}'
 cmd=[str(binary),'--filter','compare/insert_bulk_*/lin','--glob','--samples','24','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(root/name)]
 with (root/(name+'.log')).open('w') as log:subprocess.run(cmd,env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1'),stdout=log,stderr=subprocess.STDOUT,check=True)
 print(name,flush=True)
