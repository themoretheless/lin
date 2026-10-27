from pathlib import Path
import json,subprocess,hashlib,time
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());status=[]
for i in range(1,13):
 roles=['baseline','control','candidate'];shift=i%3;roles=roles[shift:]+roles[:shift]
 for role in roles:
  b=m['variants']['baseline'if role=='control'else role];assert hashlib.sha256(Path(b['binary']).read_bytes()).hexdigest()==b['binary_sha256'];start=time.time()
  with(p/f'{i:02}-{role}.jsonl').open('w')as out, (p/f'{i:02}-{role}.stderr').open('w')as err:r=subprocess.run([b['binary']],stdout=out,stderr=err)
  status.append(dict(round=i,role=role,exit_code=r.returncode,elapsed_seconds=time.time()-start));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
  rows=[json.loads(s)for s in(p/f'{i:02}-{role}.jsonl').read_text().splitlines()];assert len(rows)==1321 and rows[0]['layout'];assert sum(not r['warmup']for r in rows[1:])==1056
 print('round',i,'complete',flush=True)
