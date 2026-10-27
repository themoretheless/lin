from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};bench=Path('benches/compare.rs');bench_before=bench.read_bytes();meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},original_compare_sha256=hashlib.sha256(bench_before).hexdigest(),compiled_compare_sha256=hashlib.sha256((p/'compare.rs').read_bytes()).hexdigest(),candidate_sha256={n:hashlib.sha256((p/f'candidate-{n}.rs').read_bytes()).hexdigest()for n in names},instrumentation=False,binaries={},public_row_newtype_break=True)
assert 'pub type Row = BTreeMap<String, Cell>;'in before['store'].decode()
assert 'pub struct Row(Arc<BTreeMap<String, Cell>>);'in(p/'candidate-store.rs').read_text()
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes(before[n]if role=='baseline'else(p/f'candidate-{n}.rs').read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path(f'/tmp/lin-cow-broad-engine-{role}');shutil.copy2(exe,dest);meta['binaries'][role]=dict(path=str(dest),sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),bytes=dest.stat().st_size)
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 bench.write_bytes(bench_before);meta['source_restored']=all(Path(f'src/{n}.rs').read_bytes()==b for n,b in before.items())and bench.read_bytes()==bench_before;(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
