from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/store.rs');original=source.read_bytes();assert original==(p/'candidate-store.rs').read_bytes();m=json.loads((p/'input.json').read_text());baseline=(p/'baseline-store.rs').read_bytes();assert baseline==subprocess.check_output(['git','show',m['baseline_commit']+':src/store.rs']);status=dict(source_restored=False,baseline_regression_passed=False,performance_qualified=False)
try:
 source.write_bytes(baseline)
 test='rollback_upsert_restores_partial_and_unique_index_entries'
 with(p/'baseline-regression.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','partial_index_mutation',test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'baseline-regression.log').read_text();status.update(exit_code=r.returncode,baseline_store_sha256=hashlib.sha256(baseline).hexdigest(),baseline_regression_passed=r.returncode==0 and 'test '+test+' ... ok'in text and 'test result: ok. 1 passed;'in text)
finally:
 source.write_bytes(original);status['source_restored']=source.read_bytes()==original;(p/'baseline-regression-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['baseline_regression_passed']and status['source_restored']
