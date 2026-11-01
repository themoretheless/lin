from pathlib import Path
import subprocess,json,hashlib,shutil,sys,os
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;source=Path('src/fts.rs');bench=Path('benches/compare.rs');original=source.read_bytes();original_bench=bench.read_bytes();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
status=json.loads((p/'test-status.json').read_text());negative=json.loads((p/'mutant-status.json').read_text());assert status['passed']==459 and negative['runtime_assertion_caught'] and negative['source_restored'];assert h(source)==status['fts_sha256']
result=dict(binaries={},candidate_fts_sha256=h(source),baseline_fts_sha256=h(p/'baseline-fts.rs'),harness_sha256=h(p/'compare.rs'),target_dir=os.environ['CARGO_TARGET_DIR'])
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  source.write_bytes((p/f'{role}-fts.rs').read_bytes())
  with (p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=Path(bench_executable((p/f'build-{role}.log').read_text(),'compare')).resolve();assert exe.is_relative_to(Path(os.environ['CARGO_TARGET_DIR']).resolve())
  dest=Path('/tmp/lin-fts-pending-transfer-'+role);shutil.copy2(exe,dest);result['binaries'][role]=dict(path=str(dest),sha256=h(dest),source_sha256=h(source))
finally:
 source.write_bytes(original);bench.write_bytes(original_bench);result['source_restored']=source.read_bytes()==original and bench.read_bytes()==original_bench;(p/'provenance.json').write_text(json.dumps(result,indent=2)+'\n')
assert result['source_restored']
print('Ordinary baseline and candidate built; source restored')
