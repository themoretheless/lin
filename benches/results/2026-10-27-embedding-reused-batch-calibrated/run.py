from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent
status={}
for i in range(1,13):
 with (p/f'process-{i}.jsonl').open('w') as out,(p/f'process-{i}.log').open('w') as log:r=subprocess.run(['/tmp/lin-reused-batch-embedding-calibrated-gate',str(i)],stdout=out,stderr=log)
 status[str(i)]=r.returncode;(p/'status.json').write_text(json.dumps(status,indent=2));assert r.returncode==0
 d=[json.loads(x) for x in (p/f'process-{i}.jsonl').read_text().splitlines()];assert len(d)==4 and all(len(x['ns'])==3 and x['exact_values'] and all(len(t)==12 for t in x['ns']) for x in d)
 print(i,'complete',flush=True)
ref=json.loads((p/'provenance.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in ref['source_sha256'].items())
