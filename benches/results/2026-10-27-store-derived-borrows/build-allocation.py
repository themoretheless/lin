from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;f=Path('src/store.rs');before=f.read_bytes();example=Path('examples/store_derived_allocation.rs');assert not example.exists();meta=dict(source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},gate_sha256=hashlib.sha256((p/'allocation-gate.rs').read_bytes()).hexdigest(),binaries={})
try:
 example.write_bytes((p/'allocation-gate.rs').read_bytes())
 for role in ['baseline','candidate']:
  f.write_bytes((p/f'{role}-store.rs').read_bytes())
  with(p/f'build-allocation-{role}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','store_derived_allocation'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0;dest=Path(f'/tmp/lin-store-derived-allocation-{role}');shutil.copy2('target/release/examples/store_derived_allocation',dest);meta['binaries'][role]=dict(path=str(dest),sha256=hashlib.sha256(dest.read_bytes()).hexdigest())
finally:
 f.write_bytes(before);example.unlink(missing_ok=True);meta['source_restored']=f.read_bytes()==before;(p/'allocation-provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
