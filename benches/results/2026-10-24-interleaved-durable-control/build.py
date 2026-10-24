from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent
f=Path('examples/interleaved_durable_control.rs');assert not f.exists()
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(x):hashlib.sha256(x.read_bytes()).hexdigest() for x in sorted(Path('src').rglob('*.rs'))},'worker_sha256':hashlib.sha256((p/'worker.rs').read_bytes()).hexdigest()}
try:
 f.write_bytes((p/'worker.rs').read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','interleaved_durable_control'],stdout=log,stderr=subprocess.STDOUT)
 (p/'build-status.json').write_text(json.dumps({'exit_code':r.returncode}));assert r.returncode==0
 for v in ['baseline','candidate']:shutil.copy2('target/release/examples/interleaved_durable_control','/tmp/lin-interleaved-durable-'+v)
 meta['binary_sha256']={v:hashlib.sha256(Path('/tmp/lin-interleaved-durable-'+v).read_bytes()).hexdigest() for v in ['baseline','candidate']};assert len(set(meta['binary_sha256'].values()))==1
 (p/'provenance.json').write_text(json.dumps(meta,indent=2))
finally:f.unlink(missing_ok=True)
