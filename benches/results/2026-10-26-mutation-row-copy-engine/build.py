from pathlib import Path
import subprocess,json,hashlib,re,shutil
p=Path(__file__).resolve().parent;names=['exec','store'];before={n:Path(f'src/{n}.rs').read_text()for n in names};meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}}
for n,s in before.items():(p/f'before-{n}.rs').write_text(s)
s=before['store'];a=s.index('    pub fn fts_sync_at(');b=s.index('    /// Rebuild one collection',a);old=s[a:b];new=old.replace('            .cloned()\n','');assert new!=old;s=s[:a]+new+s[b:];candidate={'store':s}
s=before['exec'];a=s.index('            Stmt::Update {',s.index('    fn exec_stmt('));b=s.index('            Stmt::DeleteEdge',a)
old=s[a:b];new=old
for kind in ['Update','Delete']:
 needle=f'''                Ok((
                    rows.clone(),
                    None,
                    Some(Pack::{kind} {{
                        collection: collection.clone(),
                        rows,
                    }}),
                ))'''
 replace=f'''                if self.is_durable() {{
                    Ok((rows.clone(), None, Some(Pack::{kind} {{
                        collection: collection.clone(), rows,
                    }})))
                }} else {{
                    // Preserve the write marker without copying a WAL payload.
                    Ok((rows, None, Some(Pack::Batch {{ packs: Vec::new() }})))
                }}'''
 assert needle in new;new=new.replace(needle,replace)
s=s[:a]+new+s[b:];a=s.index('    fn update_cas(');b=s.index('    fn delete_rows(',a);old=s[a:b];new=old.replace('let before = self.store.collection(collection)[i].clone();','let before = &self.store.collection(collection)[i];');assert new!=old;new=new.replace('            self.check_row_fks(collection, &updated)?;','            self.check_row_fks(collection, &updated)?;\n            let before = std::mem::replace(&mut self.store.collection_mut(collection)[i], updated.clone());')
new=new.replace('            self.store.collection_mut(collection)[i] = updated.clone();','')
s=s[:a]+new+s[b:];candidate['exec']=s
for n,s in candidate.items():(p/f'candidate-{n}.rs').write_text(s)
try:
 for mode,source in [('baseline',before),('candidate',candidate)]:
  for n,s in source.items():Path(f'src/{n}.rs').write_text(s)
  with (p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,mode
  exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/f'build-{mode}.log').read_text())[1];dest=f'/tmp/lin-mutation-row-copy-{mode}';shutil.copy2(exe,dest);meta[mode+'_binary_sha256']=hashlib.sha256(Path(dest).read_bytes()).hexdigest()
finally:
 for n,s in before.items():Path(f'src/{n}.rs').write_text(s)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('both binaries built; source restored',flush=True)
