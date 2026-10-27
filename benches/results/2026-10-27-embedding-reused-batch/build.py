from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent;paths=[Path('src/lib.rs'),Path('src/embed.rs')];before={f:f.read_bytes()for f in paths};example=Path('examples/reused_batch_embedding_gate.rs');assert not example.exists()
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},'helper_sha256':hashlib.sha256((p/'helper.rs').read_bytes()).hexdigest(),'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest()}
try:
 with Path('src/embed.rs').open('a')as f:f.write('\n'+(p/'helper.rs').read_text())
 with Path('src/lib.rs').open('a')as f:f.write('\npub use embed::{diagnostic_embedding_baseline,diagnostic_embedding_reused_batch};\n')
 example.write_bytes((p/'gate.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','reused_batch_embedding_gate'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 binary=Path('/tmp/lin-reused-batch-embedding-gate');shutil.copy2('target/release/examples/reused_batch_embedding_gate',binary);meta['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
finally:
 for f,b in before.items():f.write_bytes(b)
 example.unlink(missing_ok=True);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
