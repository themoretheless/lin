from pathlib import Path
import json,subprocess,hashlib
p=Path(__file__).resolve().parent;names=['exec','persist','store','lib'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};module=Path('src/mutation_profile.rs');assert not module.exists()
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'instrumented-{n}.rs').read_bytes())
 module.write_bytes((p/'profile.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode,profile='release',features='default'),indent=2)+'\n');assert r.returncode==0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 module.unlink(missing_ok=True)
