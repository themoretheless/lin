from pathlib import Path
import subprocess,shutil,hashlib,json,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent
names=['lib','exec','store','check','fts','index'];paths=[Path(f'src/{n}.rs')for n in names]+[Path('benches/compare.rs')];original={f:f.read_bytes()for f in paths}
candidate=p
functions={'exec':[('mutation_positions',0),('update_cas',1),('delete_rows',2),('check_row_fks',3),('check_quotas',14),('exec_program',15),('run_prepared(&mut self, prepared: &Prepared)',16)],'store':[('index_insert_row',4),('fts_sync_at',5),('row_maps_sync_updated',6),('delete_rows_at',7),('move_deleted_tail',8),('index_remove_row',9),('swap_pop_columns',10),('row_maps_drop_keys',11)],'fts':[('move_row',12)],'index':[('move_at',13)]}
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'before_sha256':{str(f):hashlib.sha256(b).hexdigest()for f,b in original.items()},'probe_sha256':hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest()}
try:
 for mode in ['baseline','candidate']:
  for f,b in original.items():f.write_bytes(b)
  if mode=='candidate':
   for n in ['store','exec','check']:Path(f'src/{n}.rs').write_bytes((candidate/f'cow-source-{n}.rs').read_bytes())
  with Path('src/lib.rs').open('a')as f:f.write('\n'+(p/'probe.rs').read_text())
  for n,items in functions.items():
   f=Path(f'src/{n}.rs');text=f.read_text()
   for name,i in items:
    start=text.index('fn '+name if '('in name else 'fn '+name+'(');brace=text.index('{',start);text=text[:brace+1]+f'\n let _phase_probe=crate::phase_probe::Span::new({i});\n'+text[brace+1:]
   f.write_text(text)
  f=Path('src/exec.rs');s=f.read_text()
  edits=[('let before = self.store.collection(collection)[i].clone();','let before = {let _p=crate::phase_probe::Span::new(17);self.store.collection(collection)[i].clone()};'),('let mut updated = before.clone();','let mut updated = {let _p=crate::phase_probe::Span::new(18);before.clone()};'),('events.push(RowUndo::Update(collection.to_string(), i, before.clone()));','{let _p=crate::phase_probe::Span::new(19);events.push(RowUndo::Update(collection.to_string(), i, before.clone()));}'),('self.store.collection_mut(collection)[i] = updated.clone();','{let _p=crate::phase_probe::Span::new(20);self.store.collection_mut(collection)[i] = updated.clone();}'),('rows.clone(),\n                    None,\n                    Some(Pack::Update','{let _p=crate::phase_probe::Span::new(21);rows.clone()},\n                    None,\n                    Some(Pack::Update')]
  for a,b in edits:assert a in s,a;s=s.replace(a,b,1)
  f.write_text(s)
  f=Path('benches/compare.rs');s=original[f].decode();s=s.replace('fn main() -> airbug_bench::Result<()> {','fn main() -> airbug_bench::Result<()> {\n    let _phase_dump=lin::phase_probe::Dump;',1)
  a='black_box(s.update.run(&mut s.db).expect("lin update").done.n)';b='''let rows=s.db.store.collection("docs").len();
    lin::phase_probe::begin();
    let t=std::time::Instant::now();let handle=s.update.run(&mut s.db).expect("lin update");let prepared_ns=t.elapsed().as_nanos()as u64;let n=handle.done.n;
    let d=std::time::Instant::now();drop(handle);let drop_ns=d.elapsed().as_nanos()as u64;
    lin::phase_probe::end(rows,prepared_ns,drop_ns);black_box(n)''';assert s.count(a)==1;s=s.replace(a,b);f.write_text(s)
  meta[mode+'_instrumented_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in paths}
  for f in paths:(p/('instrumented-'+mode+'-'+f.name)).write_bytes(f.read_bytes())
  with(p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=bench_executable((p/f'build-{mode}.log').read_text(),'compare');dest=f'/tmp/lin-fresh-phase-{mode}';shutil.copy2(exe,dest);meta[mode+'_binary_sha256']=hashlib.sha256(Path(dest).read_bytes()).hexdigest()
finally:
 for f,b in original.items():f.write_bytes(b)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('Instrumented compare binaries built; all source restored')
