from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;store=Path('src/store.rs');bench=Path('benches/compare.rs');before=store.read_bytes();bench_before=bench.read_bytes();meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},original_compare_sha256=hashlib.sha256(bench_before).hexdigest(),compiled_compare_sha256=hashlib.sha256((p/'compare.rs').read_bytes()).hexdigest(),candidate_store_sha256=hashlib.sha256((p/'candidate-store.rs').read_bytes()).hexdigest(),instrumentation=False,binaries={})
assert 'pub type Row = BTreeMap<String, Cell>;' in (p/'candidate-store.rs').read_text()
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  store.write_bytes(before if role=='baseline'else(p/'candidate-store.rs').read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path(f'/tmp/lin-store-derived-engine-{role}');shutil.copy2(exe,dest);meta['binaries'][role]=dict(path=str(dest),sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),bytes=dest.stat().st_size)
finally:
 store.write_bytes(before);bench.write_bytes(bench_before);meta['source_restored']=store.read_bytes()==before and bench.read_bytes()==bench_before;(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
