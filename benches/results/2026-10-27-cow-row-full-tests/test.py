from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;candidate=p.parent/'2026-10-27-cow-row';names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(candidate/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((candidate/f'candidate-{n}.rs').read_bytes())
 with Path('src/store.rs').open('a')as f:f.write('\n'+(candidate/'oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps({'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('Check exit',r.returncode,'production source restored')

assert r.returncode == 0
