from pathlib import Path
import json,subprocess,hashlib
p=Path(__file__).resolve().parent;example=Path('examples/map_pool_allocation_audit.rs');assert not example.exists()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},probe_sha256=h(p/'probe.rs'),instrumentation='System allocator counts only; no latency/RSS claim')
try:
 example.write_bytes((p/'probe.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','map_pool_allocation_audit','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 binary=Path('target/release/examples/map_pool_allocation_audit');m['binary_sha256']=h(binary)
 for run in range(1,4):
  with(p/f'run-{run}.json').open('w')as out:subprocess.run([str(binary)],stdout=out,check=True)
finally:
 example.unlink(missing_ok=True);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
