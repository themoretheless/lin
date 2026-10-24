from pathlib import Path
import subprocess,json,hashlib,shutil,re
r=Path(__file__).resolve().parent;source=Path('src/index.rs');candidate=(r/'candidate-index.rs').read_bytes();assert source.read_bytes()==candidate;meta={}
try:
 for v in ['baseline','candidate']:
  source.write_bytes((r/'before-index.rs').read_bytes() if v=='baseline' else candidate)
  with (r/f'engine-{v}-build.log').open('w') as log:p=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0
  exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/f'engine-{v}-build.log').read_text())[1];bin=Path('/tmp/lin-bitmap-engine-'+v);shutil.copy2(exe,bin)
  meta[v]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(bin.read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
  print(v,'engine built',flush=True)
finally:source.write_bytes(candidate);(r/'engine-provenance.json').write_text(json.dumps(meta,indent=2))
