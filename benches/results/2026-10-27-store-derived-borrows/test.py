from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/store.rs');before=f.read_bytes()
try:
 f.write_bytes((p/'candidate-store.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
finally:
 f.write_bytes(before);(p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode,source_restored=f.read_bytes()==before),indent=2)+'\n')
