from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 f=Path('src/exec.rs');s=f.read_text();needle='for key in patch.keys() {';assert s.count(needle)==1
 f.write_text(s.replace(needle,'for key in patch.keys().filter(|key|key.as_str()!="hash") {',1))
 with Path('src/store.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with(p/'mutant-hash-undo.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','inplace_patch_engine_oracle'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode!=0 and 'test result: FAILED' in(p/'mutant-hash-undo.log').read_text()
 (p/'hash-undo-sensitivity-status.json').write_text(json.dumps({'missing_hash_undo_detected':True,'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('Engine oracle detects missing hash rollback; production source restored')
