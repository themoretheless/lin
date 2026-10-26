from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;example=Path('examples/persistent_hashing_cpu_gate.rs');assert not example.exists()
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},gate_sha256=hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),rustc=subprocess.check_output(['rustc','-Vv'],text=True))
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','persistent_hashing_cpu_gate'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0
 shutil.copy2('target/release/examples/persistent_hashing_cpu_gate','/tmp/lin-persistent-hashing-cpu-gate');meta['binary_sha256']=hashlib.sha256(Path('/tmp/lin-persistent-hashing-cpu-gate').read_bytes()).hexdigest()
finally:example.unlink(missing_ok=True);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
