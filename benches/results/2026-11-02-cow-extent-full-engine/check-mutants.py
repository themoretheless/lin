from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/exec.rs');original=source.read_bytes();text=original.decode();checks=[]
cases=[
 ('guard-derived-hash','if k == "hash" && !explicit_hash {','if false {',1,'update_patch_guard','update_patch_preserves_explicit_and_derived_hash_guards'),
 ('skip-explicit-hash-guard','if k == "hash" && !explicit_hash {','if k == "hash" {',1,'update_patch_guard','update_patch_preserves_explicit_and_derived_hash_guards'),
 ('lost-prefix-journal','let prefix = std::mem::replace(&mut self.undo, Undo::None);','let _discarded = std::mem::replace(&mut self.undo, Undo::None); let prefix = Undo::None;',1,'transaction_schema_undo','generated_ids_are_reusable_after_schema_snapshot_rollback'),
 ('missing-checkpoint','self.checkpoint_undo();','();',2,'transaction_schema_undo','schema_after_rows_rollback_restores_rows_metadata_and_constraints'),
 ('reversed-checkpoint-order','self.store.mem_restore(snapshot);\n                self.rollback(prefix, None);','self.rollback(prefix, None);\n                self.store.mem_restore(snapshot);',1,'transaction_schema_undo','repeated_schema_changes_preserve_journal_order_and_abort_state'),
]
try:
 for label,old,new,count,target,test in cases:
  assert text.count(old)==count,(label,text.count(old));mutant=text.replace(old,new).encode();source.write_bytes(mutant);(p/(label+'-mutant.rs')).write_bytes(mutant)
  with(p/(label+'-runtime.log')).open('w')as log:
   r=subprocess.run(['cargo','test','--release','--features','experimental-extent-wal','--test',target,test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/(label+'-runtime.log')).read_text()
  rejected=r.returncode==101 and 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and 'test '+test+' ... FAILED'in log
  checks.append(dict(label=label,exit_code=r.returncode,test=test,mutant_sha256=hashlib.sha256(mutant).hexdigest(),compiled_runtime_rejected=rejected))
  assert rejected,label
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_restored=source.read_bytes()==original),indent=2)+'\n')
assert len(checks)==5
