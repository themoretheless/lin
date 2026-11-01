from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;source=Path('src/store.rs');original=source.read_bytes();text=original.decode();a=text.index('    pub(crate) fn restore_row_owned(');b=text.index('\n    pub fn restore_rows(',a);block=text[a:b];needle='        let _ = self.index_insert_row(collection, idx, &restored);';assert block.count(needle)==1
status=dict(source_restored=False,compiled_runtime_rejected=False,performance_qualified=False)
try:
 mutant=(text[:a]+block.replace(needle,'        let _ = (&restored, collection, idx);')+text[b:]).encode();source.write_bytes(mutant);(p/'missing-index-upsert-mutant.rs').write_bytes(mutant)
 test='rollback_upsert_restores_partial_and_unique_index_entries'
 with(p/'missing-index-upsert-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','partial_index_mutation',test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
 log=(p/'missing-index-upsert-runtime.log').read_text();status.update(exit_code=r.returncode,mutant_sha256=hashlib.sha256(mutant).hexdigest(),compiled_runtime_rejected=r.returncode==101 and 'Finished `release` profile'in log and 'running 1 test'in log and 'test '+test+' ... FAILED'in log and 'test result: FAILED. 0 passed; 1 failed;'in log)
finally:
 source.write_bytes(original);status['source_restored']=source.read_bytes()==original;(p/'mutant-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['source_restored']and status['compiled_runtime_rejected']
