from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'legacy-provenance.json').read_text());snapshot=Path(m['snapshot']);example=snapshot/'examples/column_batch_reader.rs'
assert all(sha((snapshot/f).read_bytes())==h for f,h in m['source_sha256'].items())
example.write_bytes((p/'legacy-reader.rs').read_bytes());env=dict(os.environ,CARGO_TARGET_DIR=m['target_dir'],CARGO_BUILD_JOBS='2')
with(p/'legacy-driver-rebuild.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--locked','--example','column_batch_reader','--message-format=json-render-diagnostics'],cwd=snapshot,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
assert all(sha((snapshot/f).read_bytes())==h for f,h in m['source_sha256'].items());m.update(driver_sha256=sha(example.read_bytes()),binary_sha256=sha(Path(m['binary']).read_bytes()),driver_f32_bit_oracle=True);(p/'legacy-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
