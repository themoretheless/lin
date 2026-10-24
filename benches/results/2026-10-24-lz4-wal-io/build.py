from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent
f=Path('examples/lz4_wal_io_gate.rs');assert not f.exists()
m={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(x):hashlib.sha256(x.read_bytes()).hexdigest() for x in sorted(Path('src').rglob('*.rs'))},'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),'cargo_lock_sha256':hashlib.sha256(Path('Cargo.lock').read_bytes()).hexdigest()}
try:
 f.write_bytes((p/'gate.rs').read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--features','compress-lz4','--example','lz4_wal_io_gate'],stdout=log,stderr=subprocess.STDOUT)
 (p/'build-status.json').write_text(json.dumps({'exit_code':r.returncode}));assert r.returncode==0
 shutil.copy2('target/release/examples/lz4_wal_io_gate','/tmp/lin-lz4-wal-io-gate');m['binary_sha256']=hashlib.sha256(Path('/tmp/lin-lz4-wal-io-gate').read_bytes()).hexdigest();(p/'provenance.json').write_text(json.dumps(m,indent=2))
finally:f.unlink(missing_ok=True)
