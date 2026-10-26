from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary='/tmp/lin-bitmap-emptiness-cpu';assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==meta['binary_sha256'];status=[]
for i in range(1,13):
 with(p/f'process-{i}.jsonl').open('w')as out,(p/f'process-{i}.log').open('w')as log:r=subprocess.run([binary,str(i)],stdout=out,stderr=log)
 status.append(dict(process=i,exit_code=r.returncode,load=os.getloadavg()));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0;print(i,0,flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
