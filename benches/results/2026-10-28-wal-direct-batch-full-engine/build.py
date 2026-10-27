from pathlib import Path
import hashlib,json,shutil,subprocess,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;persist=Path('src/persist.rs');bench=Path('benches/compare.rs');original=persist.read_bytes();bench_before=bench.read_bytes();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},original_compare_sha256=h(bench),compiled_compare_sha256=h(p/'compare.rs'),candidate_persist_sha256=h(p/'candidate-persist.rs'),instrumentation=False,binaries={})
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  persist.write_bytes(original if role=='baseline'else(p/'candidate-persist.rs').read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path(f'/tmp/lin-wal-direct-batch-full-engine-{role}');shutil.copy2(exe,dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest))
finally:
 persist.write_bytes(original);bench.write_bytes(bench_before);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items())and h(bench)==m['original_compare_sha256'];(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
