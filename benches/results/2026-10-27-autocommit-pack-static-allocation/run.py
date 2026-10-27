from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status=[]
for round in range(1,4):
 for mode in ['baseline','candidate']:
  binary=Path('/tmp/lin-autocommit-pack-static-allocation-'+mode)
  assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
  with(p/f'{round:02}-{mode}.jsonl').open('w')as out,(p/f'{round:02}-{mode}.log').open('w')as log:r=subprocess.run([str(binary)],stdout=out,stderr=log)
  status.append(dict(round=round,mode=mode,exit_code=r.returncode));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
