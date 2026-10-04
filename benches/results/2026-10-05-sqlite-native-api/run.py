from pathlib import Path
import subprocess,sys,json,hashlib
root=Path(__file__).resolve().parent
binary=Path('/tmp/lin-current-peer-read-matrix')
(root/'binary-sha256.json').write_text(json.dumps({'lin':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
for n in [1000,10000]:
 name='rows-'+str(n)
 cmd=['.bench-venv/bin/python','scripts/bench-peers.py','--engines','lin','sqlite','--cases','insert_native','--rows',str(n),'--samples','24','--repeats','3','--require-wins','--lin-binary',str(binary),'--output',str(root/name)]
 with (root/(name+'.log')).open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 print(n,r.returncode,flush=True)
 if r.returncode not in [0,3]:sys.exit(r.returncode)
