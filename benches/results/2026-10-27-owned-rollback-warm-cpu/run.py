from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status=[]
for mode in ['baseline','candidate']:assert hashlib.sha256(Path(f'/tmp/lin-owned-rollback-warm-cpu-{mode}').read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
for i in range(1,7):
 modes=['baseline','control','candidate'];shift=i%3;modes=modes[shift:]+modes[:shift]
 for mode in modes:
  binary='/tmp/lin-owned-rollback-warm-cpu-'+('candidate'if mode=='candidate'else'baseline');name=f'{i:02}-{mode}'
  with(p/(name+'.jsonl')).open('w')as out,(p/(name+'.log')).open('w')as log:r=subprocess.run([binary],stdout=out,stderr=log)
  status.append(dict(run=name,exit_code=r.returncode));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
  print(name,flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
