from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;names=['store','fts'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 with Path('src/fts.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'status.json').write_text(json.dumps({'exit_code':r.returncode})+'\n');assert r.returncode==0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
