from pathlib import Path
import subprocess,shutil,hashlib,json,sys,re
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['lib','exec','store','check','fts','index'];paths=[Path(f'src/{n}.rs')for n in names]+[Path('benches/compare.rs')];before={f:f.read_bytes()for f in paths}
functions={'exec':[('mutation_positions',0),('update_cas',1),('delete_rows',2),('check_patch_fks',3),('check_quotas',14),('exec_program',15),('run_prepared(&mut self, prepared: &Prepared)',16),('record_row',17)],'store':[('index_insert_row',4),('row_maps_sync_updated',6),('delete_rows_at',7),('move_deleted_tail',8),('index_remove_row',9),('swap_pop_columns',10),('row_maps_drop_keys',11)],'fts':[('sync_row',5),('move_row',12)],'index':[('move_at',13),('key_of',22),('insert_key',23)]}
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'before_sha256':{str(f):hashlib.sha256(b).hexdigest()for f,b in before.items()},'probe_sha256':hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest()}
try:
 for n in ['store','exec','check','index']:Path(f'src/{n}.rs').write_bytes((p/f'map-source-{n}.rs').read_bytes())
 with Path('src/lib.rs').open('a')as f:f.write('\n'+(p/'probe.rs').read_text())
 with Path('src/lib.rs').open('a')as f:f.write('\n'+(p/'copy.rs').read_text())
 for n,items in functions.items():
  f=Path(f'src/{n}.rs');s=f.read_text()
  for name,i in items:
   a=s.index('fn '+name)if '('in name else re.search(r'\bfn '+re.escape(name)+r'(?:<[^>]*>)?\s*\(',s).start();b=s.index('{',a);s=s[:b+1]+f'\n let _phase_probe=crate::phase_probe::Span::new({i});\n'+s[b+1:]
  f.write_text(s)
 f=Path('src/exec.rs');s=f.read_text();a=s.index('impl RowDelta');a=s.index('fn capture(',a);b=s.index('{',a);s=s[:b+1]+'\n let _phase_probe=crate::phase_probe::Span::new(18);\n'+s[b+1:]
 start=s.index('fn update_cas(');a=s.index('        for i in &idxs {',start);b=s.index('        let mut effective_patch = record_row(record, now);',a);s=s[:a]+'        { let _cas=crate::phase_probe::Span::new(24);\n'+s[a:b]+'        }\n'+s[b:]
 a=s.index('            let had_embedding =',start);b=s.index('            if let Undo::Rows',a);s=s[:a]+'            let (had_embedding, identity_before, fts_before) = { let _meta=crate::phase_probe::Span::new(20);\n'+s[a:b]+'            (had_embedding, identity_before, fts_before) };\n'+s[b:]
 a=s.index('            let live = &mut self.store.collection_mut(collection)[i];',start);b=s.index('            let updated = self.store.collection(collection)[i].clone();',a);s=s[:a]+'            { let _patch=crate::phase_probe::Span::new(19);\n'+s[a:b]+'            }\n'+s[b:]
 old='let updated = self.store.collection(collection)[i].clone();';new='let updated = {let _copy=crate::phase_probe::Span::new(21);crate::row_copy_probe::copy(&self.store.collection(collection)[i])};';assert s.count(old)==1;s=s.replace(old,new);f.write_text(s)
 f=Path('benches/compare.rs');s=before[f].decode();s=s.replace('fn main() -> airbug_bench::Result<()> {','fn main() -> airbug_bench::Result<()> {\n let _phase_dump=lin::phase_probe::Dump;lin::row_copy_probe::preflight();let mode=match std::env::var("LIN_DIAGNOSTIC_ROW_COPY").unwrap_or_else(|_|"std".into()).as_str(){"std"=>0,"iter"=>1,"staged"=>2,_=>panic!("invalid diagnostic copy mode")};lin::row_copy_probe::set_mode(mode);',1)
 for kind in ['update','delete']:
  old=f'black_box(s.{kind}.run(&mut s.db).expect("lin {kind}").done.n)';new=f'''let rows=s.db.store.collection("docs").len();lin::phase_probe::begin();
    let t=std::time::Instant::now();let handle=s.{kind}.run(&mut s.db).expect("lin {kind}");let prepared_ns=t.elapsed().as_nanos()as u64;let n=handle.done.n;
    let d=std::time::Instant::now();drop(handle);let drop_ns=d.elapsed().as_nanos()as u64;
    lin::phase_probe::end("{kind}",rows,prepared_ns,drop_ns);black_box(n)''';assert s.count(old)==1;s=s.replace(old,new)
 old='let prepared_ns=t.elapsed().as_nanos()as u64;let n=handle.done.n;';new='let prepared_ns=t.elapsed().as_nanos()as u64;let n=handle.done.n;';assert s.count(old)==2
 # Insert output check only into UPDATE callback, with its unique end label.
 a=s.index('fn write_update_lin(');b=s.index('fn write_delete_lin(',a);part=s[a:b];assert part.count(old)==1;part=part.replace(old,old+'assert_eq!(handle.rows.as_slice(),std::slice::from_ref(s.db.store.get_by_id("docs",&s.probe_id).unwrap()));');s=s[:a]+part+s[b:]
 f.write_text(s);meta['instrumented_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in paths}
 for f in paths:(p/('instrumented-'+f.name)).write_bytes(f.read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=bench_executable((p/'build.log').read_text(),'compare');dest='/tmp/lin-row-copy-diagnostic';shutil.copy2(exe,dest);meta['binary_sha256']=hashlib.sha256(Path(dest).read_bytes()).hexdigest()
finally:
 for f,b in before.items():f.write_bytes(b)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('Map output diagnostic built; all source restored')
