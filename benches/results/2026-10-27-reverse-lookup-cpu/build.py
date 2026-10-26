from pathlib import Path
import subprocess,shutil,hashlib,json
p=Path(__file__).resolve().parent;f=Path('examples/reverse_lookup_gate.rs');assert not f.exists()
try:
 f.write_bytes((p/'gate.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','reverse_lookup_gate'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 shutil.copy2('target/release/examples/reverse_lookup_gate','/tmp/lin-reverse-lookup')
finally:f.unlink(missing_ok=True)
(p/'provenance.json').write_text(json.dumps({'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashlib.sha256(Path('/tmp/lin-reverse-lookup').read_bytes()).hexdigest(),'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest()},indent=2)+'\n')
