from pathlib import Path
import subprocess,json,hashlib,shutil,re
r=Path(__file__).resolve().parent
source=Path('src/embed.rs');before=source.read_bytes();status={};provenance={}
assert before==(r/'before-embed.rs').read_bytes()
try:
 for variant in ['baseline','candidate']:
  source.write_bytes(before if variant=='baseline' else (r/'candidate-embed.rs').read_bytes())
  if variant=='candidate':
   with (r/'workspace.log').open('w') as log:p=subprocess.run(['cargo','test','--workspace'],stdout=log,stderr=subprocess.STDOUT)
   status['workspace']=p.returncode;assert p.returncode==0
  with (r/f'{variant}-build.log').open('w') as log:p=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  status[variant]=p.returncode;assert p.returncode==0
  exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/f'{variant}-build.log').read_text())[1]
  binary=Path('/tmp/lin-embedding-sparse-fused-'+variant);shutil.copy2(exe,binary)
  provenance[variant]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'compiler':subprocess.check_output(['rustc','-Vv'],text=True)}
  print(variant,'built',flush=True)
finally:
 source.write_bytes(before)
 (r/'checks.json').write_text(json.dumps(status,indent=2))
 (r/'provenance.json').write_text(json.dumps(provenance,indent=2))
