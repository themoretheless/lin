from pathlib import Path
import subprocess,json,hashlib,shutil,re,os
r=Path(__file__).resolve().parent;names=['exec','store','persist'];meta={};checks={}
base=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert all(Path(f'src/{n}.rs').read_bytes()==(r/f'before-{n}.rs').read_bytes() for n in names)
try:
 for variant in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((r/f'{"before" if variant=="baseline" else "candidate"}-{n}.rs').read_bytes())
  if variant=='candidate':
   with (r/'workspace.log').open('w') as log:p=subprocess.run(['cargo','test','--workspace'],stdout=log,stderr=subprocess.STDOUT)
   checks['workspace']=p.returncode;(r/'checks.json').write_text(json.dumps(checks,indent=2));assert p.returncode==0
  with (r/f'{variant}-build.log').open('w') as log:p=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  checks[variant]=p.returncode;(r/'checks.json').write_text(json.dumps(checks,indent=2));assert p.returncode==0
  exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/f'{variant}-build.log').read_text())[1];binary=Path('/tmp/lin-wal-overlap-port-'+variant);shutil.copy2(exe,binary)
  meta[variant]={'base_commit':base,'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
  (r/'provenance.json').write_text(json.dumps(meta,indent=2));print(variant,'built',flush=True)
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((r/f'candidate-{n}.rs').read_bytes())
