from pathlib import Path
import subprocess,json,hashlib,platform,os,time
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
assert not(p/'process-status.json').exists(),'No implicit unchanged rerun'
assert not os.environ.get('DYLD_INSERT_LIBRARIES')and not os.environ.get('LD_PRELOAD')
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),instrumentation=False),indent=2)+'\n')
records=[]
for repeat in range(1,13):
 roles=['baseline','control','candidate'];shift=repeat%3;roles=roles[shift:]+roles[:shift]
 for role in roles:
  binary=m['binaries']['baseline'if role=='control'else role];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
  start=time.time()
  with(p/f'{repeat:02}-{role}.jsonl').open('w')as output,(p/f'{repeat:02}-{role}.log').open('w')as log:r=subprocess.run([binary['path']],stdout=output,stderr=log)
  records.append(dict(round=repeat,mode=role,exit_code=r.returncode,binary_sha256=binary['sha256'],elapsed_seconds=time.time()-start));(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0
 print(repeat,'complete',flush=True)
assert len(records)==36
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==v for f,v in m['source_sha256'].items())
