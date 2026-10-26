from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;before={n:Path(f'src/{n}.rs').read_bytes()for n in ['exec','store']}
try:
 for n in before:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 f=Path('src/exec.rs');s=f.read_text();needle='before.get(key).cloned()';assert s.count(needle)==1
 f.write_text(s.replace(needle,'before.get(key).cloned().or(Some(Cell::Null))')+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','patch_undo_oracle'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode!=0 and 'test result: FAILED' in(p/'mutant.log').read_text()
 (p/'sensitivity-status.json').write_text(json.dumps({'missing_added_field_undo_detected':True,'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('Delta oracle detects missing added-field rollback; sources restored')
