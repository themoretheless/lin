from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent;files={'neon_sparse_count_cpu.rs':'gate.rs','neon_count_reference.rs':'reference.rs','neon_count_candidate.rs':'candidate.rs'}
assert all(not Path('examples',x).exists() for x in files)
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},gate_sha256={x:hashlib.sha256((p/x).read_bytes()).hexdigest() for x in files.values()},rustc=subprocess.check_output(['rustc','-Vv'],text=True))
try:
 for target,source in files.items():Path('examples',target).write_bytes((p/source).read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','neon_sparse_count_cpu'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0
 shutil.copy2('target/release/examples/neon_sparse_count_cpu','/tmp/lin-neon-sparse-count-cpu');meta['binary_sha256']=hashlib.sha256(Path('/tmp/lin-neon-sparse-count-cpu').read_bytes()).hexdigest()
finally:
 for target in files:Path('examples',target).unlink(missing_ok=True)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2))
