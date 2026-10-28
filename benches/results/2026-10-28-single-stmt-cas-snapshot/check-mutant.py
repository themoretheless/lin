from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/exec.rs');before=f.read_bytes();source=before.decode()
old='let need_snap = stmts.len() > 1 && stmts.iter().any(|s| {';assert source.count(old)==1
mutant=source.replace(old,'let need_snap = false && stmts.iter().any(|s| {');(p/'mutant-exec.rs').write_text(mutant)
try:
 f.write_text(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','single_stmt_cas','multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything','--color','never','--','--exact'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();assert r.returncode!=0 and 'running 1 test'in text and 'multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything ... FAILED'in text and 'panicked'in text
finally:
 f.write_bytes(before)
 result=dict(mutant='disable all pack-start conflict tracking including multi-statement programs',runtime_rejected=r.returncode!=0,source_restored=f.read_bytes()==before,original_exec_sha256=hashlib.sha256(before).hexdigest(),mutant_exec_sha256=hashlib.sha256(mutant.encode()).hexdigest());(p/'mutant-validation.json').write_text(json.dumps(result,indent=2)+'\n')
print('Incorrect multi-statement snapshot bypass rejected at runtime; source restored')
