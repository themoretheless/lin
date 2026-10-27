from pathlib import Path
import subprocess,json,shutil
p=Path(__file__).resolve().parent;example=Path('examples/lock_inheritance_gate.rs');assert not example.exists();names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};candidate=p.parent/'2026-10-27-cow-inplace-patch';status=[]
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for mode in ['baseline','cow_inplace']:
  for n in names:Path(f'src/{n}.rs').write_bytes(before[n]if mode=='baseline'else(candidate/f'candidate-{n}.rs').read_bytes())
  with(p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','build','--example','lock_inheritance_gate'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  for i in range(1,4):
   with(p/f'{mode}-{i}.jsonl').open('w')as out,(p/f'{mode}-{i}.log').open('w')as log:r=subprocess.run(['target/debug/examples/lock_inheritance_gate'],stdout=out,stderr=log)
   status.append(dict(mode=mode,process=i,exit_code=r.returncode));print(mode,i,r.returncode,flush=True)
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);(p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
