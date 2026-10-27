from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored'] and all(h(f)==d for f,d in m['source_sha256'].items());status=[]
for repeat in range(1,4):
 for n in [1000,10000,100000]:
  for role in (['baseline','candidate'] if repeat%2 else ['candidate','baseline']):
   b=m['binaries'][role];assert h(b['path'])==b['sha256'];name=f'{role}-{n}-{repeat}'
   with(p/(name+'.json')).open('w') as out,(p/(name+'.log')).open('w') as log:r=subprocess.run([b['path'],str(n)],stdout=out,stderr=log)
   status.append(dict(repeat=repeat,rows=n,role=role,exit_code=r.returncode,binary_sha256=h(b['path'])));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0,name
   d=json.loads((p/(name+'.json')).read_text());assert d['rows']==n and d['exact_all_rows_readback'];print(name,flush=True)
assert len(status)==18 and all(h(f)==d for f,d in m['source_sha256'].items())
