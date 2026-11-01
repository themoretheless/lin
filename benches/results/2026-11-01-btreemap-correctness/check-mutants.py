from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;files={Path(n):Path(n).read_bytes()for n in ['src/exec.rs','src/store.rs']};store=files[Path('src/store.rs')].decode();a=store.index('        // Appended vector nodes must not outlive');b=store.index('        self.next_id = mark.next_id;',a)
cases=[
 ('missing-delete-vector','src/store.rs','            self.hnsw_insert_slab(collection, p, std::slice::from_ref(doomed));\n','',1,['--lib'],'rollback_restores_hnsw_after_delete'),
 ('stale-append-vector','src/store.rs',store[a:b],'',1,['--lib'],'append_rollback_removes_hnsw_nodes_at_truncated_slots'),
 ('missing-ordered-append','src/exec.rs','events.push(RowUndo::Append(self.db.store.append_mark()));','();',1,['--test','transaction_tests'],'rollback_interleaved_append_delete_preserves_rows_and_keys'),
 ('lost-prefix-journal','src/exec.rs','let prefix = std::mem::replace(&mut self.undo, Undo::None);','let _discarded = std::mem::replace(&mut self.undo, Undo::None); let prefix = Undo::None;',1,['--test','transaction_schema_undo'],'generated_ids_are_reusable_after_schema_snapshot_rollback'),
 ('missing-checkpoint','src/exec.rs','self.checkpoint_undo();','();',2,['--test','transaction_schema_undo'],'schema_after_rows_rollback_restores_rows_metadata_and_constraints'),
 ('reversed-checkpoint-order','src/exec.rs','self.store.mem_restore(snapshot);\n                self.rollback(prefix, None);','self.rollback(prefix, None);\n                self.store.mem_restore(snapshot);',1,['--test','transaction_schema_undo'],'repeated_schema_changes_preserve_journal_order_and_abort_state'),
];checks=[]
try:
 for label,name,old,new,count,args,test in cases:
  source=Path(name);text=files[source].decode();assert text.count(old)==count,label;mutant=text.replace(old,new).encode();source.write_bytes(mutant);(p/(label+'-mutant.rs')).write_bytes(mutant)
  with(p/(label+'-runtime.log')).open('w')as log:r=subprocess.run(['cargo','test','--release',*args,test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/(label+'-runtime.log')).read_text();rejected=r.returncode==101 and 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and bool(re.search(r'test [^\n]*'+re.escape(test)+r' \.\.\. FAILED',log))
  source.write_bytes(files[source]);checks.append(dict(label=label,source_file=name,test=test,exit_code=r.returncode,mutant_sha256=hashlib.sha256(mutant).hexdigest(),compiled_runtime_rejected=rejected,source_restored=source.read_bytes()==files[source]));assert rejected,label
finally:
 for source,data in files.items():source.write_bytes(data)
 (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,sources_restored=all(f.read_bytes()==b for f,b in files.items())),indent=2)+'\n')
assert len(checks)==6
