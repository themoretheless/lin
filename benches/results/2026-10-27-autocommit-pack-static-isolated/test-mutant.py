from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 f=Path('src/exec.rs');t=f.read_text();assert t.count('if recover_output_rows {')==1;f.write_text(t.replace('if recover_output_rows {','if false && recover_output_rows {'))
 with Path('src/store.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with Path('src/index.rs').open('a')as f:f.write('\n'+(p/'index-oracle.rs').read_text())
 with(p/'mutant-empty-output.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','autocommit_payload_oracle'],stdout=log,stderr=subprocess.STDOUT)
 (p/'mutant-empty-output-status.json').write_text(json.dumps({'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('Check exit',r.returncode,'production source restored')

assert r.returncode != 0
