from pathlib import Path
import subprocess,shutil,json,hashlib
p=Path(__file__).resolve().parent
paths=[Path('src/lib.rs'),Path('src/exec.rs'),Path('src/store.rs'),Path('src/fts.rs'),Path('src/index.rs')]
original={f:f.read_bytes() for f in paths}
example=Path('examples/mutation_stage_profile.rs'); assert not example.exists()
module='''
/// Temporary diagnostic instrumentation; excluded from production source.
pub mod mutation_stage_profile {
 use std::sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed};
 pub static ACTIVE:AtomicBool=AtomicBool::new(false);
 pub static NS:[AtomicU64;14]=[const {AtomicU64::new(0)};14];
 pub struct Span(usize,Option<std::time::Instant>);
 impl Span {pub fn new(i:usize)->Self{Self(i,ACTIVE.load(Relaxed).then(std::time::Instant::now))}}
 impl Drop for Span {fn drop(&mut self){if let Some(t)=self.1{NS[self.0].fetch_add(t.elapsed().as_nanos() as u64,Relaxed);}}}
}
'''
functions={Path('src/exec.rs'):[('mutation_positions',0),('update_cas',1),('delete_rows',2),('check_row_fks',3)],Path('src/store.rs'):[('index_insert_row',4),('fts_sync_at',5),('row_maps_sync_updated',6),('delete_rows_at',7),('move_deleted_tail',8),('index_remove_row',9),('swap_pop_columns',10),('row_maps_drop_keys',11)],Path('src/fts.rs'):[('move_row',12)],Path('src/index.rs'):[('move_at',13)]}
try:
 Path('src/lib.rs').write_bytes(original[Path('src/lib.rs')]+module.encode())
 for f,items in functions.items():
  text=original[f].decode()
  for name,i in items:
   start=text.index('fn '+name+'('); brace=text.index('{',start)
   text=text[:brace+1]+f'\n let _stage_profile=crate::mutation_stage_profile::Span::new({i});\n'+text[brace+1:]
  f.write_text(text)
 source=(p.parent/'2026-10-27-fts-swap-delete-allocation/gate.rs').read_text()
 source=source.replace('ACTIVE.store(true,Relaxed);','for v in &lin::mutation_stage_profile::NS {v.store(0,Relaxed);}\nlin::mutation_stage_profile::ACTIVE.store(true,Relaxed);\nACTIVE.store(true,Relaxed);')
 source=source.replace('ACTIVE.store(false,Relaxed);','ACTIVE.store(false,Relaxed);\nlin::mutation_stage_profile::ACTIVE.store(false,Relaxed);\nprintln!("STAGES rows={} kind={} ns={:?}",n,kind,lin::mutation_stage_profile::NS.iter().map(|v|v.load(Relaxed)).collect::<Vec<_>>());')
 example.write_text(source)
 (p/'instrumented-example.rs').write_text(source)
 for f in paths:(p/('instrumented-'+f.name)).write_bytes(f.read_bytes())
 with (p/'build.log').open('w') as log:
  r=subprocess.run(['cargo','build','--release','--example','mutation_stage_profile'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 shutil.copy2('target/release/examples/mutation_stage_profile','/tmp/lin-mutation-stage-profile-v2')
finally:
 for f,b in original.items():f.write_bytes(b)
 example.unlink(missing_ok=True)
 (p/'source-sha256.json').write_text(json.dumps({str(f):hashlib.sha256(b).hexdigest()for f,b in original.items()},indent=2)+'\n')
print('Built instrumented diagnostic; production sources restored')
