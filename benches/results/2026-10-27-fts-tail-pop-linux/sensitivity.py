from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent
files={n:Path(f'src/{n}.rs')for n in ['fts','store']}
before={n:f.read_bytes()for n,f in files.items()}
assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n,f in files.items():f.write_bytes((p/f'candidate-{n}.rs').read_bytes())
 f=files['fts'];text=f.read_text();assert 'base.pop(); true' in text
 f.write_text(text.replace('base.pop(); true','/* intentional stale tail */ true',1)+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','fused_swap_delete_oracle'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode!=0
 assert 'test result: FAILED' in (p/'mutant.log').read_text()
 (p/'sensitivity-status.json').write_text(json.dumps({'intentional_stale_tail_detected':True,'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,f in files.items():f.write_bytes(before[n])
print('Oracle detects stale-tail mutant; sources restored')
