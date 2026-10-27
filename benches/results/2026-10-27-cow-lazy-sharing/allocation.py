from pathlib import Path
import json,subprocess,hashlib,shutil
p=Path(__file__).resolve().parent;frozen=p.parent/'2026-10-27-cow-broad-engine';names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};example=Path('examples/cow_lazy_allocation_probe.rs');assert not example.exists()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},rustc=subprocess.check_output(['rustc','-Vv'],text=True),probe_sha256=h(p/'allocation-probe.rs'),roles={},performance_qualified=False)
try:
 example.write_bytes((p/'allocation-probe.rs').read_bytes())
 for role in ['main','frozen_cow','lazy_cow']:
  for n in names:Path(f'src/{n}.rs').write_bytes(before[n]if role=='main'else((p if role=='lazy_cow'and n in ['exec','store']else frozen)/f'candidate-{n}.rs').read_bytes())
  applied={n:h(f'src/{n}.rs')for n in names}
  with(p/f'allocation-build-{role}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','cow_lazy_allocation_probe','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  binary=Path('target/release/examples/cow_lazy_allocation_probe');saved=Path(f'/tmp/lin-cow-lazy-allocation-{role}');shutil.copy2(binary,saved);m['roles'][role]=dict(source_sha256=applied,binary_sha256=h(saved),binary_path=str(saved))
  for run in range(1,4):
   with(p/f'allocation-{role}-{run}.json').open('w')as out,(p/f'allocation-{role}-{run}.stderr.log').open('w')as err:subprocess.run([str(saved)],stdout=out,stderr=err,check=True)
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'allocation-status.json').write_text(json.dumps(m,indent=2)+'\n')
