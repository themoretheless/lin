from pathlib import Path
import hashlib,json,shutil,subprocess,sys,os
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text());tests=json.loads((p/'test-status.json').read_text());negative=json.loads((p/'mutant-status.json').read_text())
assert tests['exit_code']==0 and tests['release_passed']==459 and tests['source_sha256']==m['source_sha256']
assert negative['source_restored']and negative['compiled_runtime_rejected']
assert all(sha(Path(n).read_bytes())==h for n,h in m['source_sha256'].items())
store=Path('src/store.rs');bench=Path('benches/compare.rs');before={f:f.read_bytes()for f in [store,bench]}
assert (p/'baseline-store.rs').read_bytes()==subprocess.check_output(['git','show',m['baseline_commit']+':src/store.rs'])
assert (p/'candidate-store.rs').read_bytes()==before[store]
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=m['source_sha256'],original_compare_sha256=sha(before[bench]),compiled_compare_sha256=sha((p/'compare.rs').read_bytes()),binaries={},instrumentation=False,source_restored=False)
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  store.write_bytes((p/(role+'-store.rs')).read_bytes())
  with(p/('build-'+role+'.log')).open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=Path(bench_executable((p/('build-'+role+'.log')).read_text(),'compare')).resolve();target=Path(os.environ['CARGO_TARGET_DIR']).resolve();assert exe.is_relative_to(target)
  dest=Path('/tmp/lin-rollback-index-upsert-'+role);shutil.copy2(exe,dest);meta['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),store_sha256=sha(store.read_bytes()),cargo_target_dir=str(target))
finally:
 for f,data in before.items():f.write_bytes(data)
 meta['source_restored']=all(sha(Path(n).read_bytes())==h for n,h in m['source_sha256'].items())and bench.read_bytes()==before[bench];(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']and len(meta['binaries'])==2
