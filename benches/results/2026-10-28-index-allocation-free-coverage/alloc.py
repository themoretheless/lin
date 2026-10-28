from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;source=Path('src/index.rs');before=source.read_bytes();example=Path('examples/index_coverage_alloc.rs');assert not example.exists();library=Path('src/lib.rs');library_before=library.read_bytes();shim=Path('src/__coverage_alloc_probe.rs');assert not shim.exists();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(candidate_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),baseline_reference='a7d4b02c3',source_sha256=hashes,probe_sha256=hashlib.sha256((p/'alloc.rs').read_bytes()).hexdigest(),binaries={},performance_qualified=False)
try:
 shim.write_bytes((p/'alloc-shim.rs').read_bytes());library.write_bytes(library_before+b'\n#[doc(hidden)]\n#[path="__coverage_alloc_probe.rs"]\npub mod __coverage_alloc_probe;\n');m['diagnostic_shim_sha256']=hashlib.sha256(shim.read_bytes()).hexdigest();m['compiled_lib_sha256']=hashlib.sha256(library.read_bytes()).hexdigest();m['diagnostic_shim_only']=True
 example.write_bytes((p/'alloc.rs').read_bytes())
 for role in ['baseline','candidate']:
  source.write_bytes(subprocess.check_output(['git','show','a7d4b02c3:src/index.rs'])if role=='baseline'else before)
  with(p/f'alloc-build-{role}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','index_coverage_alloc','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  found=[]
  for line in(p/f'alloc-build-{role}.log').read_text().splitlines():
   try:q=json.loads(line)
   except ValueError:continue
   if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='index_coverage_alloc'and q.get('executable'):found.append(q['executable'])
  assert len(found)==1;binary=Path('/tmp/lin-index-coverage-alloc-'+role);shutil.copy2(found[0],binary);m['binaries'][role]=dict(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),index_sha256=hashlib.sha256(source.read_bytes()).hexdigest())
  for shape in ['leaf','dotted','partial','compound']:
   for repeat in range(3):
    with(p/f'{role}-{shape}-{repeat}.json').open('w')as out,(p/f'{role}-{shape}-{repeat}.stderr').open('w')as log:r=subprocess.run([str(binary),shape],stdout=out,stderr=log)
    assert r.returncode==0
finally:
 source.write_bytes(before);library.write_bytes(library_before);shim.unlink(missing_ok=True);example.unlink(missing_ok=True);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'alloc-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['all_sources_restored']
print('24 isolated allocation-count diagnostics complete; no timing claim')
