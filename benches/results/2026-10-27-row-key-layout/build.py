from pathlib import Path
import subprocess,shutil,hashlib,json
p=Path(__file__).resolve().parent;f=Path('examples/row_key_layout_gate.rs');assert not f.exists()
try:
 f.write_bytes((p/'gate.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','row_key_layout_gate'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 shutil.copy2('target/release/examples/row_key_layout_gate','/tmp/lin-row-key-layout')
finally:f.unlink(missing_ok=True)
(p/'provenance.json').write_text(json.dumps({'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashlib.sha256(Path('/tmp/lin-row-key-layout').read_bytes()).hexdigest(),'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest()},indent=2)+'\n')
