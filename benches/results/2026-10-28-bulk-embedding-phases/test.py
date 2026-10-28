from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;names=['exec','lib','embed'];before={name:Path(f'src/{name}.rs').read_bytes()for name in names};profile=Path('src/bulk_profile.rs');assert not profile.exists();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(checkout_source_sha256=hashes,profile_reference='a7d4b02c3',performance_qualified=False)
try:
 for name in names:Path(f'src/{name}.rs').write_bytes((p/f'instrumented-{name}.rs').read_bytes())
 profile.write_bytes((p/'profile.rs').read_bytes());m['compiled_sources_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
 with(p/'full-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 m['exit']=r.returncode;assert r.returncode==0
finally:
 for name,data in before.items():Path(f'src/{name}.rs').write_bytes(data)
 profile.unlink(missing_ok=True);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'full-status.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['all_sources_restored']
print('instrumented full release suite passed; all current sources restored')
