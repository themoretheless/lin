from pathlib import Path
import subprocess,json,hashlib,shutil,re
r=Path(__file__).resolve().parent;names=['persist','store','exec'];candidate={name:Path(f'src/{name}.rs').read_bytes() for name in names};meta={}
assert all(candidate[name]==(r/f'candidate-{name}.rs').read_bytes() for name in names)
try:
 for variant in ['baseline','candidate']:
  for name in names:Path(f'src/{name}.rs').write_bytes((r/f'before-{name}.rs').read_bytes() if variant=='baseline' else candidate[name])
  with (r/f'{variant}-build.log').open('w') as log:p=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0
  exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/f'{variant}-build.log').read_text())[1];binary=Path('/tmp/lin-wal-overlap-'+variant);shutil.copy2(exe,binary)
  meta[variant]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
  print(variant,'built',flush=True)
finally:
 for name in names:Path(f'src/{name}.rs').write_bytes(candidate[name])
 (r/'provenance.json').write_text(json.dumps(meta,indent=2))
