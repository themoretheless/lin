from pathlib import Path
import subprocess,json,hashlib,shutil
r=Path(__file__).resolve().parent;names=['exec'];candidate={n:Path(f'src/{n}.rs').read_bytes() for n in names};meta={};example=Path('examples/small_neon_overlap_interleaved.rs');assert not example.exists()
try:
 example.write_bytes((r/'interleaved-worker.rs').read_bytes())
 for v in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((r/f'{"before" if v=="baseline" else "candidate"}-{n}.rs').read_bytes())
  with (r/f'interleaved-{v}-build.log').open('w') as log:p=subprocess.run(['cargo','build','--release','--example','small_neon_overlap_interleaved'],stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0
  binary='/tmp/lin-small-neon-overlap-interleaved-ab-'+v;shutil.copy2('target/release/examples/small_neon_overlap_interleaved',binary)
  meta[v]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes(candidate[n])
 example.unlink(missing_ok=True)
 (r/'interleaved-provenance.json').write_text(json.dumps(meta,indent=2))
for v in ['baseline','candidate']:shutil.copy2('/tmp/lin-small-neon-overlap-interleaved-ab-baseline','/tmp/lin-small-neon-overlap-interleaved-aa-'+v)
reference=Path('benches/results/2026-10-24-interleaved-durable-control/run.py').read_text()
for mode in ['aa','ab']:
 p=r/f'interleaved-{mode}';p.mkdir();prefix='/tmp/lin-small-neon-overlap-interleaved-'+mode+'-';(p/'run.py').write_text(reference.replace('/tmp/lin-interleaved-durable-',prefix))
 (p/'provenance.json').write_text(json.dumps({'source_sha256':meta['candidate']['source_sha256'],'baseline_source_sha256':meta['baseline']['source_sha256'],'binary_sha256':{v:hashlib.sha256(Path(prefix+v).read_bytes()).hexdigest() for v in ['baseline','candidate']}},indent=2))
