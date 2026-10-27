from pathlib import Path
import json,subprocess
p=Path(__file__).resolve().parent;old=p.parent/'2026-10-27-cow-broad-engine';target=Path('tests/cow_owning_adapters_probe.rs');assert not target.exists();names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names}
try:
 target.write_bytes((p/'owning-tests.rs').read_bytes())
 for n in names:Path(f'src/{n}.rs').write_bytes(((p if n=='store'else old)/f'candidate-{n}.rs').read_bytes())
 for n,oracle in [('store','oracle.rs'),('index','index-oracle.rs')]:
  with Path(f'src/{n}.rs').open('a')as f:f.write('\n'+(old/oracle).read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode),indent=2)+'\n');assert r.returncode==0
finally:
 target.unlink(missing_ok=True)
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
