from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;cow=p.parent/'2026-10-27-cow-bitmap-prefix-isolated-gate';files=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in files};example=Path('examples/row_api_compat_probe.rs');assert not example.exists();status=[];meta=dict(source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},candidate_sha256={n:hashlib.sha256((cow/f'candidate-{n}.rs').read_bytes()).hexdigest()for n in files})
try:
 for role in ['baseline','cow']:
  for n in files:Path(f'src/{n}.rs').write_bytes(before[n]if role=='baseline'else(cow/f'candidate-{n}.rs').read_bytes())
  for case in ['positive','exact-assignment','owning-map-method','into-map']:
   example.write_bytes((p/f'{case}.rs').read_bytes())
   with(p/f'{role}-{case}.log').open('w')as log:r=subprocess.run(['cargo','check','--example','row_api_compat_probe'],stdout=log,stderr=subprocess.STDOUT)
   expected=role=='baseline'or case=='positive';assert(r.returncode==0)==expected,(role,case,r.returncode)
   status.append(dict(role=role,case=case,exit_code=r.returncode,expected_pass=expected));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');print(role,case,r.returncode,flush=True)
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);meta['source_restored']=all(Path(f'src/{n}.rs').read_bytes()==b for n,b in before.items());(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
