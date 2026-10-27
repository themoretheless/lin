from pathlib import Path
import subprocess,shutil,hashlib,json,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;paths=[Path('src/lib.rs'),Path('src/exec.rs'),Path('src/store.rs'),Path('benches/compare.rs')];before={f:f.read_bytes()for f in paths}
functions={'exec':[('insert_bulk',0),('maybe_embed_rows',1),('maintain_insert_slab',2),('run_prepared(&mut self, prepared: &Prepared)',7),('append_batch_shape_rows',8)],'store':[('index_insert_slab',3),('fts_insert_slab',4),('hnsw_insert_slab',5),('row_maps_register_slab',6)]}
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'before_sha256':{str(f):hashlib.sha256(b).hexdigest()for f,b in before.items()},'stages':functions,'diagnostic_only':True}
try:
 with Path('src/lib.rs').open('a')as f:f.write('\n'+(p/'probe.rs').read_text())
 for n,items in functions.items():
  f=Path(f'src/{n}.rs');text=f.read_text()
  for name,i in items:
   needle='fn '+name if '('in name else 'fn '+name+'(';assert text.count(needle)==1,(n,name);a=text.index(needle);b=text.index('{',a);text=text[:b+1]+f'\n let _phase=crate::phase_probe::Span::new({i});\n'+text[b+1:]
  if n=='exec':
   a=text.index('        let mut batch_ids:',text.index('fn insert_bulk('));b=text.index('        self.maybe_embed_rows(collection, &mut built);',a)
   text=text[:a]+'        {let _validation=crate::phase_probe::Span::new(9);\n'+text[a:b]+'        }\n'+text[b:]
  f.write_text(text)
 f=Path('benches/compare.rs');text=f.read_text();text=text.replace('fn main() -> airbug_bench::Result<()> {','fn main() -> airbug_bench::Result<()> {\n let _phase_dump=lin::phase_probe::Dump;',1)
 old='ins.prepared.run(&mut ins.db).expect("lin insert").done.n';assert text.count(old)==1
 new='lin::phase_probe::begin();let t=std::time::Instant::now();let handle=ins.prepared.run(&mut ins.db).expect("lin insert");let ns=t.elapsed().as_nanos()as u64;let n=handle.done.n;let t=std::time::Instant::now();drop(handle);let drop_ns=t.elapsed().as_nanos()as u64;lin::phase_probe::end("insert_bulk",n,ns,drop_ns);n'
 text=text.replace(old,new);f.write_text(text)
 meta['instrumented_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in paths}
 for f in paths:(p/('instrumented-'+f.name)).write_bytes(f.read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=bench_executable((p/'build.log').read_text(),'compare');binary=Path('/tmp/lin-main-bulk-profile');shutil.copy2(exe,binary);meta['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
finally:
 for f,b in before.items():f.write_bytes(b)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
