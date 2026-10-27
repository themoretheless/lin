from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'baseline-cow-{n}.rs').read_bytes())
 with Path('src/store.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with Path('src/index.rs').open('a')as f:f.write('\n'+(p/'index-oracle.rs').read_text())
 with(p/'layout-baseline.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','scalar_posting_layout','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 (p/'layout-baseline-status.json').write_text(json.dumps({'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('Check exit',r.returncode,'production source restored')

assert r.returncode == 0
