from pathlib import Path
import subprocess,json,hashlib,os,time
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status={}
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items());assert hashlib.sha256(Path('/tmp/lin-persistent-hashing-cpu-gate').read_bytes()).hexdigest()==meta['binary_sha256']
for i in range(1,13):
 name=f'process-{i}';load=os.getloadavg()
 with (p/(name+'.jsonl')).open('w') as out,(p/(name+'.log')).open('w') as log:r=subprocess.run(['/tmp/lin-persistent-hashing-cpu-gate'],stdout=out,stderr=log)
 status[name]=dict(exit_code=r.returncode,load_before=load,load_after=os.getloadavg());(p/'status.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True);assert r.returncode==0
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
