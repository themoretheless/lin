from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/store.rs');before=source.read_text()
needle='            fts.sync_row(idx, &current, &restored);'
assert before.count(needle)==1
mutant=before.replace(needle,'            let _ = fts; // deliberately omit rollback FTS synchronization')
(p/'mutant-store.rs').write_text(mutant)
runtime_exit=None;caught=False
try:
 source.write_text(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','held_row_isolated_and_body_hash_fts_maps_restore_on_rollback','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 runtime_exit=r.returncode
 text=(p/'mutant-runtime.log').read_text()
 caught=r.returncode==101 and 'test result: FAILED.'in text and 'running 1 test'in text and ' ... FAILED'in text
 assert caught,'Expected compiled runtime regression failure'
finally:
 source.write_text(before)
 (p/'mutant-status.json').write_text(json.dumps(dict(runtime_exit=runtime_exit,compiled_runtime_rejected=caught,source_restored=source.read_text()==before,source_sha256=hashlib.sha256(before.encode()).hexdigest()),indent=2)+'\n')
