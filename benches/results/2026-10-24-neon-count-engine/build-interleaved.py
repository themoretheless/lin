from pathlib import Path
import subprocess,hashlib,shutil,json
p=Path(__file__).resolve().parent;f=Path('src/persist.rs');candidate=(p/'candidate-persist.rs').read_bytes();assert f.read_bytes()==(p/'before-persist.rs').read_bytes()
example=Path('examples/neon_count_interleaved.rs');assert not example.exists();meta={}
try:
 example.write_bytes((p/'interleaved-worker.rs').read_bytes())
 for v in ['candidate','baseline']:
  f.write_bytes(candidate if v=='candidate' else (p/'before-persist.rs').read_bytes())
  with (p/f'interleaved-{v}-build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','neon_count_interleaved'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  binary='/tmp/lin-neon-count-interleaved-ab-'+v;shutil.copy2('target/release/examples/neon_count_interleaved',binary)
  meta[v]={'source_sha256':{str(s):hashlib.sha256(s.read_bytes()).hexdigest() for s in sorted(Path('src').rglob('*.rs'))},'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
finally:f.write_bytes((p/'before-persist.rs').read_bytes());example.unlink(missing_ok=True)
assert meta['baseline']['rustc']==meta['candidate']['rustc'];(p/'interleaved-build-provenance.json').write_text(json.dumps(meta,indent=2))
for v in ['baseline','candidate']:shutil.copy2('/tmp/lin-neon-count-interleaved-ab-baseline','/tmp/lin-neon-count-interleaved-aa-'+v)
reference=Path('benches/results/2026-10-24-interleaved-durable-control/run.py').read_text()
for name in ['aa','ab']:
 q=p/f'interleaved-{name}';q.mkdir(exist_ok=False);prefix='/tmp/lin-neon-count-interleaved-'+name+'-'
 (q/'run.py').write_text(reference.replace('/tmp/lin-interleaved-durable-',prefix))
 (q/'provenance.json').write_text(json.dumps({'source_kind':'NEON candidate binaries; worktree restored to baseline before timing','source_sha256':meta['baseline']['source_sha256'],'baseline_source_sha256':meta['baseline']['source_sha256'],'binary_sha256':{v:hashlib.sha256(Path(prefix+v).read_bytes()).hexdigest() for v in ['baseline','candidate']}},indent=2))
