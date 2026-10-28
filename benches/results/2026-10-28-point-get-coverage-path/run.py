from pathlib import Path
import json,subprocess,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'));from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;files=[Path('src/index.rs'),Path('src/lib.rs'),Path('benches/compare.rs')];before={f:f.read_bytes()for f in files};hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(checkout_source_sha256=hashes,compare_sha256=hashlib.sha256((p/'compare.rs').read_bytes()).hexdigest(),binaries={},performance_qualified=False,counter_only=True)
try:
 Path('src/lib.rs').write_bytes(before[Path('src/lib.rs')]+b'\n#[doc(hidden)]\npub fn __index_coverage_probe_calls()->u64{index::__index_coverage_probe_calls()}\n');Path('benches/compare.rs').write_bytes((p/'compare.rs').read_bytes())
 for role,ref in [('baseline','a7d4b02c3'),('candidate','884ebeb81')]:
  index=subprocess.check_output(['git','show',ref+':src/index.rs']);needle=b'pub fn index_covers_pred(pred: &Pred, uses: &[IndexUse]) -> bool {\n';assert index.count(needle)==1;index=index.replace(needle,needle+b'    INDEX_COVERAGE_PROBE_CALLS.fetch_add(1,std::sync::atomic::Ordering::Relaxed);\n');index+=b'\nstatic INDEX_COVERAGE_PROBE_CALLS:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);\npub(crate) fn __index_coverage_probe_calls()->u64{INDEX_COVERAGE_PROBE_CALLS.load(std::sync::atomic::Ordering::Relaxed)}\n';Path('src/index.rs').write_bytes(index);m['binaries'][role]=dict(source_reference=ref,original_index_sha256=hashlib.sha256(subprocess.check_output(['git','show',ref+':src/index.rs'])).hexdigest(),counter_index_sha256=hashlib.sha256(index).hexdigest(),counter_lib_sha256=hashlib.sha256(Path('src/lib.rs').read_bytes()).hexdigest())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');binary=Path('/tmp/lin-point-get-coverage-'+role);shutil.copy2(exe,binary);m['binaries'][role].update(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
  for shape in ['point_get','filter_eq','filter_range']:
   for repeat in range(2):
    with(p/f'{role}-{shape}-{repeat}.json').open('w')as out,(p/f'{role}-{shape}-{repeat}.stderr').open('w')as log:r=subprocess.run([str(binary),shape],stdout=out,stderr=log)
    assert r.returncode==0
finally:
 for f,data in before.items():f.write_bytes(data)
 m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items())and all(f.read_bytes()==data for f,data in before.items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['all_sources_restored']
print('12 source-path counter diagnostics complete; every source restored; no timing claim')
