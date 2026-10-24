from pathlib import Path
import subprocess,json,re,shutil,hashlib
p=Path(__file__).resolve().parent
assert Path('src/exec.rs').read_bytes()==(p/'candidate-exec.rs').read_bytes()
status={};hashes={}
try:
 for v in ['baseline','candidate']:
  Path('src/exec.rs').write_bytes((p/('before-exec.rs' if v=='baseline' else 'candidate-exec.rs')).read_bytes())
  with (p/('compare-build-'+v+'.log')).open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
  status[v]=r.returncode;(p/'compare-build-status.json').write_text(json.dumps(status))
  if r.returncode:raise SystemExit(r.returncode)
  executable=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/('compare-build-'+v+'.log')).read_text())[1]
  binary=Path('/tmp/lin-bulk-hash-compare-'+v);shutil.copy2(executable,binary)
  hashes[v]=hashlib.sha256(binary.read_bytes()).hexdigest();print(v,'built',flush=True)
 (p/'compare-binary-sha256.json').write_text(json.dumps(hashes,indent=2))
finally:Path('src/exec.rs').write_bytes((p/'candidate-exec.rs').read_bytes())
