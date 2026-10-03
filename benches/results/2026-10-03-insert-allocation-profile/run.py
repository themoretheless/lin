from pathlib import Path
import subprocess,hashlib,json
root=Path(__file__).resolve().parent
binary=Path('/tmp/lin-insert-allocation-profile')
(root/'binary-sha256.json').write_text(json.dumps({'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
for i in range(1,4):
 with (root/f'process-{i}.log').open('w') as log:subprocess.run([str(binary)],stdout=log,stderr=subprocess.STDOUT,check=True)
 print(i,flush=True)
