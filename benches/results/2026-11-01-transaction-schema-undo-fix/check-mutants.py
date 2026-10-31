from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/exec.rs');original=source.read_bytes();text=original.decode();checks=[]
cases=[
 ('lost-initial-append-boundary','let append = mark.clone();','let append = self.db.store.append_mark();',1,'generated_ids_are_reusable_after_schema_snapshot_rollback'),
 ('missing-ordered-snapshot','events.push(RowUndo::Snapshot(Box::new(self.db.store.mem_backup())));','();',1,'schema_after_rows_rollback_restores_rows_metadata_and_constraints'),
 ('forward-undo-journal','for event in events.into_iter().rev() {','for event in events.into_iter() {',2,'repeated_schema_snapshots_preserve_journal_order_and_abort_state'),
]
try:
 for label,old,new,count,test in cases:
  assert text.count(old)==count,(label,text.count(old));mutant=text.replace(old,new).encode();source.write_bytes(mutant);(p/(label+'-mutant.rs')).write_bytes(mutant)
  with(p/(label+'-runtime.log')).open('w')as log:
   r=subprocess.run(['cargo','test','--release','--test','transaction_schema_undo',test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/(label+'-runtime.log')).read_text()
  rejected=r.returncode==101 and 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and 'test '+test+' ... FAILED'in log
  checks.append(dict(label=label,exit_code=r.returncode,test=test,mutant_sha256=hashlib.sha256(mutant).hexdigest(),compiled_runtime_rejected=rejected))
  assert rejected,label
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_restored=source.read_bytes()==original),indent=2)+'\n')
assert len(checks)==3
