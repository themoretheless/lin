from pathlib import Path
import subprocess,shutil,json,hashlib
p=Path(__file__).resolve().parent
persist=Path('src/persist.rs');candidate=(p/'candidate-persist.rs').read_bytes();assert persist.read_bytes()==candidate
cold=Path('examples/preallocation_cold.rs');crash=Path('examples/preallocation_crash.rs');assert not cold.exists() and not crash.exists()
meta={}
try:
 cold.write_bytes((p/'cold-worker.rs').read_bytes());crash.write_bytes((p/'crash-worker.rs').read_bytes())
 with (p/'candidate-helper-build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','preallocation_cold','--example','preallocation_crash'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 for name in ['cold','crash']:shutil.copy2('target/release/examples/preallocation_'+name,'/tmp/lin-preallocation-'+name+'-candidate')
 meta['candidate_source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path('src').rglob('*.rs'))}
 persist.write_bytes((p/'before-persist.rs').read_bytes())
 with (p/'baseline-helper-build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','preallocation_cold'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 shutil.copy2('target/release/examples/preallocation_cold','/tmp/lin-preallocation-cold-baseline')
 meta['baseline_source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path('src').rglob('*.rs'))}
finally:
 persist.write_bytes(candidate);cold.unlink(missing_ok=True);crash.unlink(missing_ok=True)
meta['binary_sha256']={name:hashlib.sha256(Path('/tmp/lin-preallocation-'+name).read_bytes()).hexdigest() for name in ['cold-baseline','cold-candidate','crash-candidate']};(p/'helper-provenance.json').write_text(json.dumps(meta,indent=2))
