from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/exec.rs');before=f.read_bytes();assert before==(p/'before-exec.rs').read_bytes()
try:
 s=(p/'candidate-exec.rs').read_text();needle='UndoField::Shared(Arc::from(key.as_str()))';assert s.count(needle)==1
 f.write_text(s.replace(needle,'UndoField::Shared(Arc::from("truncated"))')+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','compact_undo_oracle'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode!=0 and 'test result: FAILED' in(p/'mutant.log').read_text()
 (p/'sensitivity-status.json').write_text(json.dumps({'long_name_corruption_detected':True,'exit_code':r.returncode},indent=2)+'\n')
finally:f.write_bytes(before)
print('Corrupted long field detected; source restored')
