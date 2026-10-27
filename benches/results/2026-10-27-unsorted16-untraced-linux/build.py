from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},'candidate_sha256':{n:hashlib.sha256((p/f'candidate-{n}.rs').read_bytes()).hexdigest()for n in names},'one_binary_all_roles':True,'instrumentation':False}
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 assert 'pub type Row = BTreeMap<String, Cell>;'in Path('src/store.rs').read_text()
 for f in [Path('src/lib.rs'),Path('src/exec.rs'),Path('benches/compare.rs')]:
  text=f.read_text();assert all(marker not in text for marker in ['phase_probe','allocation_probe','row_copy_probe','#[global_allocator]'])
 meta['compiled_source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0;exe=bench_executable((p/'build.log').read_text(),'compare');dest=Path('/tmp/lin-unsorted16-untraced');shutil.copy2(exe,dest);meta['binary_sha256']=hashlib.sha256(dest.read_bytes()).hexdigest()
finally:
 for n,data in before.items():Path(f'src/{n}.rs').write_bytes(data)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
