from pathlib import Path
import json,subprocess
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes()
try:
 f.write_bytes((p/'candidate-index.rs').read_bytes()+b'\n'+(p/'oracle.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode),indent=2)+'\n');assert r.returncode==0
finally:f.write_bytes(before)
