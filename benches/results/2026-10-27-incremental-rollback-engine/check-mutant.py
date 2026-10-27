from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};test=Path('tests/incremental_owned_rollback.rs');assert not test.exists();h=lambda b:hashlib.sha256(b).hexdigest()
try:
 for f in files:f.write_bytes((p/('candidate-'+f.name)).read_bytes())
 s=files[1].read_text();needle='fts.sync_row(idx, &current, &row);';assert s.count(needle)==1;files[1].write_text(s.replace(needle,'let _ = (fts, idx, &current, &row);'));test.write_bytes((p/'regression.rs').read_bytes())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','incremental_owned_rollback','incremental_rollback_preserves_fts_after_success_and_index_failure','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant.log').read_text();assert r.returncode!=0 and 'test result: FAILED'in text and 'panicked at'in text and 'error[E'not in text;(p/'mutant-status.json').write_text(json.dumps(dict(exit_code=r.returncode,expected_failure=True,runtime_assertion_caught=True,mutant='omitted inverse FTS synchronization'),indent=2)+'\n')
finally:
 for f,data in before.items():f.write_bytes(data)
 test.unlink(missing_ok=True)
