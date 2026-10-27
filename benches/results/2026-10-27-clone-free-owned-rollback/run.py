from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items());status=[];assert all(h(b['path'])==b['sha256']for b in m['binaries'].values())
for mode,binary in m['binaries'].items():
 with(p/f'smoke-{mode}.jsonl').open('w')as out,(p/f'smoke-{mode}.log').open('w')as log:r=subprocess.run([binary['path']],env=dict(os.environ,LIN_ROLLBACK_SMOKE='1'),stdout=out,stderr=log)
 assert r.returncode==0,(mode,'smoke')
for repeat in range(1,13):
 modes=['baseline','control','candidate'];shift=repeat%3;modes=modes[shift:]+modes[:shift]
 for mode in modes:
  binary=m['binaries']['candidate'if mode=='candidate'else'baseline'];name=f'{repeat:02}-{mode}';load=os.getloadavg()
  with(p/(name+'.jsonl')).open('w')as out,(p/(name+'.log')).open('w')as log:child=subprocess.Popen([binary['path']],stdout=out,stderr=log);pid=child.pid;code=child.wait()
  status.append(dict(repeat=repeat,mode=mode,pid=pid,exit_code=code,load_before=load,load_after=os.getloadavg(),binary_sha256=h(binary['path'])));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert code==0;records=[json.loads(line)for line in(p/(name+'.jsonl')).read_text().splitlines()];assert len(records)==96;assert all(h(f)==v for f,v in m['source_sha256'].items());assert h(binary['path'])==binary['sha256'];print(name,flush=True)
print('Complete36processes/3456diagnostic observations;full-source gate not evaluated')
