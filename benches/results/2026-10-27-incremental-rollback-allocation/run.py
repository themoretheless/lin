from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items());status=[];records=[]
for repeat in range(1,4):
 for role in (['baseline','candidate']if repeat%2 else['candidate','baseline']):
  b=m['binaries'][role];assert h(b['path'])==b['sha256'];name=f'{repeat:02}-{role}'
  with(p/(name+'.jsonl')).open('w')as out,(p/(name+'.log')).open('w')as log:child=subprocess.Popen([b['path']],stdout=out,stderr=log);pid=child.pid;code=child.wait()
  assert code==0;rows=[json.loads(s)for s in(p/(name+'.jsonl')).read_text().splitlines()];assert len(rows)==36;records.extend(dict(repeat=repeat,role=role,**r)for r in rows);status.append(dict(repeat=repeat,role=role,pid=pid,exit_code=code,binary_sha256=h(b['path'])));(p/'records.json').write_text(json.dumps(records,indent=2)+'\n');(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert h(b['path'])==b['sha256']and all(h(f)==v for f,v in m['source_sha256'].items())
print('Complete6processes/216allocation windows;no latency/RSS claim')
