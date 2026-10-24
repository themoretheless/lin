from pathlib import Path
import subprocess,hashlib,shutil,json
r=Path(__file__).resolve().parent;source=Path('src/index.rs');candidate=source.read_bytes();assert candidate==(r/'candidate-index.rs').read_bytes()
example=Path('examples/index_spilled_key_stream.rs');assert not example.exists();meta={};checks={}
try:
 example.write_bytes((r/'worker.rs').read_bytes())
 with (r/'workspace.log').open('w') as log:p=subprocess.run(['cargo','test','--workspace'],stdout=log,stderr=subprocess.STDOUT)
 checks['workspace']=p.returncode;assert p.returncode==0
 for name in ['baseline','candidate']:
  source.write_bytes((r/'before-index.rs').read_bytes() if name=='baseline' else candidate)
  with (r/f'{name}-build.log').open('w') as log:p=subprocess.run(['cargo','build','--release','--example','index_spilled_key_stream'],stdout=log,stderr=subprocess.STDOUT)
  checks[name]=p.returncode;assert p.returncode==0
  binary=Path('/tmp/lin-index-spilled-key-stream-'+name);shutil.copy2('target/release/examples/index_spilled_key_stream',binary)
  meta[name]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
  print(name,'built',flush=True)
finally:
 source.write_bytes(candidate);example.unlink(missing_ok=True)
 (r/'checks.json').write_text(json.dumps(checks,indent=2));(r/'provenance.json').write_text(json.dumps(meta,indent=2))
