from pathlib import Path
import subprocess,json,hashlib,re,shutil
p=Path(__file__).resolve().parent; f=Path('src/exec.rs');before=f.read_text();assert not (p/'before-exec.rs').exists();(p/'before-exec.rs').write_text(before)
s=before
helper='''
thread_local! { static MUT_PHASES:std::cell::RefCell<Option<Vec<(&'static str,u128)>>> = const {std::cell::RefCell::new(None)}; }
fn mut_measure<T>(name:&'static str,f:impl FnOnce()->T)->T {let t=Instant::now();let v=f();let n=t.elapsed().as_nanos();MUT_PHASES.with(|p|{if let Some(v)=p.borrow_mut().as_mut(){v.push((name,n));}});v}
struct MutProfile {active:bool,kind:&'static str,n:usize,t:Instant}
impl Drop for MutProfile {fn drop(&mut self){if self.active {let total=self.t.elapsed().as_nanos();let phases=MUT_PHASES.with(|p|p.borrow_mut().take().unwrap());eprintln!("MUTATION_PHASE {}",serde_json::json!({"kind":self.kind,"rows":self.n,"total_ns":total,"phases":phases}));}}}
'''
s=helper+s
needle='    pub fn run_prepared(&mut self, prepared: &Prepared) -> Result<Handle, Error> {\n'
assert s.count(needle)==1
s=s.replace(needle,needle+'''        let kind=match prepared.stmts.first(){Some(Stmt::Update{..})=>"update",Some(Stmt::Delete{..})=>"delete",_=>"other"};
        let active=kind!="other";if active{MUT_PHASES.with(|p|*p.borrow_mut()=Some(Vec::with_capacity(32)));}
        let _mutation_profile=MutProfile{active,kind,n:self.store.collection("docs").len(),t:Instant::now()};
''')
replacements={
 'self.exec_program(&prepared.stmts, &mut undo, true)':'mut_measure("exec_program",||self.exec_program(&prepared.stmts, &mut undo, true))',
 'self.mutation_positions(collection, pred, now)':'mut_measure("lookup",||self.mutation_positions(collection, pred, now))',
 'self.mutation_positions(collection, Some(pred), now)':'mut_measure("lookup",||self.mutation_positions(collection, Some(pred), now))',
 'let before = self.store.collection(collection)[i].clone();':'let before = mut_measure("before_clone",||self.store.collection(collection)[i].clone());',
 'let mut updated = before.clone();':'let mut updated = mut_measure("updated_clone",||before.clone());',
 'self.store.collection_mut(collection)[i] = updated.clone();':'mut_measure("store_clone",||self.store.collection_mut(collection)[i] = updated.clone());',
 'self.store.index_insert_row(collection, i, &updated)?;':'mut_measure("indexes",||self.store.index_insert_row(collection, i, &updated))?;',
 'self.store.fts_sync_at(collection, i, &before);':'mut_measure("fts",||self.store.fts_sync_at(collection, i, &before));',
 'self.store\n                .row_maps_sync_updated(collection, i, &before, &updated);':'mut_measure("row_maps",||self.store.row_maps_sync_updated(collection, i, &before, &updated));',
 'self.store.delete_rows_at(collection, &idxs);':'mut_measure("delete_derived",||self.store.delete_rows_at(collection, &idxs));',
}
for a,b in replacements.items():assert a in s,a;s=s.replace(a,b)
# Time the pack/result row clone only in update/delete statement arms.
a=s.index('            Stmt::Update {',s.index('    fn exec_stmt('));b=s.index('            Stmt::DeleteEdge',a);section=s[a:b];assert section.count('rows.clone(),')==2;s=s[:a]+section.replace('rows.clone(),','mut_measure("pack_result_clone",||rows.clone()),')+s[b:]
(p/'instrumented-exec.rs').write_text(s);meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),before_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')})
try:
 f.write_text(s)
 with (p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0
 exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'build.log').read_text())[1];binary='/tmp/lin-mutation-phase-profile';shutil.copy2(exe,binary);meta['binary_sha256']=hashlib.sha256(Path(binary).read_bytes()).hexdigest()
finally:
 f.write_text(before);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('profile built; source restored',flush=True)
