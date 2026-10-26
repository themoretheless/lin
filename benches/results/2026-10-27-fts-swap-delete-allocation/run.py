from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status=[]
for mode in ['baseline','candidate']:assert hashlib.sha256(Path(f'/tmp/lin-fts-swap-delete-allocation-{mode}').read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
for i in range(1,4):
 for mode in (['baseline','candidate']if i%2 else['candidate','baseline']):
  name=f'{mode}-{i}'
  with (p/(name+'.jsonl')).open('w')as out,(p/(name+'.log')).open('w')as log:r=subprocess.run([f'/tmp/lin-fts-swap-delete-allocation-{mode}'],stdout=out,stderr=log)
  status.append(dict(run=name,exit_code=r.returncode));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0;print(name,0,flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
