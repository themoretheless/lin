from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'));from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['exec','lib','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};bench=Path('benches/compare.rs');bench_before=bench.read_bytes();module=Path('src/bulk_profile.rs');assert not module.exists();h=lambda b:hashlib.sha256(b).hexdigest();spec=json.loads((p/'input.json').read_text());tests=json.loads((p/'full-status.json').read_text());assert tests['exit']==0 and tests['all_sources_restored']
assert all(h(Path(f).read_bytes())==d for f,d in spec['checkout_source_sha256'].items())
meta=dict(profile_reference=spec['profile_reference'],restored_checkout_sha256=spec['checkout_source_sha256'],profile_source_sha256=spec['profile_source_sha256'],binaries={},performance_qualified=False,observer_only=True)
try:
 for role in ['original','instrumented']:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'{role}-{n}.rs').read_bytes())
  if role=='instrumented':module.write_bytes((p/'profile.rs').read_bytes())
  else:module.unlink(missing_ok=True)
  expected=dict(spec['profile_source_sha256'])
  for n in names:expected[f'src/{n}.rs']=h((p/f'{role}-{n}.rs').read_bytes())
  if role=='instrumented':expected['src/bulk_profile.rs']=h((p/'profile.rs').read_bytes())
  actual={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')};assert actual==expected
  if role=='instrumented':assert tests['compiled_sources_sha256']==actual
  bench.write_bytes((p/f'{role}-compare.rs').read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path('/tmp/lin-cow-bulk-phases-'+role);shutil.copy2(exe,dest);meta['binaries'][role]=dict(path=str(dest),sha256=h(dest.read_bytes()),source_sha256=actual,compare_sha256=h(bench.read_bytes()))
finally:
 for n,data in before.items():Path(f'src/{n}.rs').write_bytes(data)
 bench.write_bytes(bench_before);module.unlink(missing_ok=True);meta['all_sources_restored']=all(h(Path(f).read_bytes())==d for f,d in spec['checkout_source_sha256'].items())and bench.read_bytes()==bench_before;(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['all_sources_restored']
print('ordinary and phase-instrumented binaries built; every checkout source restored')
