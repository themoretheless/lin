from pathlib import Path
import subprocess,json
from runtime import capture,env_for
p=Path(__file__).resolve().parent;capture(p);names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names}
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 with Path('src/store.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with Path('src/index.rs').open('a')as f:f.write('\n'+(p/'index-oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],env=env_for('candidate'),stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode,runtime_unsorted_limit=16),indent=2)+'\n');assert r.returncode==0
finally:
 for n,data in before.items():Path(f'src/{n}.rs').write_bytes(data)
