from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;s=p.parent/'2026-10-27-autocommit-pack-static';names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};status=[]
assert all(b==(s/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for mode in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((s/f'{"before"if mode=="baseline"else"candidate"}-{n}.rs').read_bytes())
  with Path('src/exec.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
  with(p/f'{mode}-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','autocommit_boundary_oracle'],stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(variant=mode,exit_code=r.returncode));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
