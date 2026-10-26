from pathlib import Path
import subprocess,hashlib,json,shutil
r=Path(__file__).resolve().parent;example=Path('examples/wal_persistent_boundaries.rs');assert not example.exists();meta={}
try:
 example.write_text((r/'interleaved-worker.rs').read_text().replace('assert!(n==1000 || n==10000);','assert!([4096,16384].contains(&n));'))
 for v in ['baseline','candidate']:
  for n in ['exec','persist','store']:Path(f'src/{n}.rs').write_bytes((r/f'{"before" if v=="baseline" else "final-candidate"}-{n}.rs').read_bytes())
  with (r/f'boundary-{v}-build.log').open('w') as log:p=subprocess.run(['cargo','build','--release','--example','wal_persistent_boundaries'],stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0
  binary='/tmp/lin-wal-persistent-boundary-ab-'+v;shutil.copy2('target/release/examples/wal_persistent_boundaries',binary)
  meta[v]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest()}
finally:
 for n in ['exec','persist','store']:Path(f'src/{n}.rs').write_bytes((r/f'final-candidate-{n}.rs').read_bytes())
 example.unlink(missing_ok=True)
(r/'boundary-provenance.json').write_text(json.dumps(meta,indent=2))
for v in ['baseline','candidate']:shutil.copy2('/tmp/lin-wal-persistent-boundary-ab-baseline','/tmp/lin-wal-persistent-boundary-aa-'+v)
reference=Path('benches/results/2026-10-24-interleaved-durable-control/run.py').read_text().replace('for n in [1000,10000]:','for n in [4096,16384]:')
status={}
for mode in ['aa','ab','aa-s2','ab-s2']:
 base=mode.split('-')[0];p=r/f'boundary-{mode}';p.mkdir();prefix='/tmp/lin-wal-persistent-boundary-'+base+'-'
 (p/'run.py').write_text(reference.replace('/tmp/lin-interleaved-durable-',prefix))
 (p/'provenance.json').write_text(json.dumps({'source_sha256':meta['candidate']['source_sha256'],'baseline_source_sha256':meta['baseline']['source_sha256'],'binary_sha256':{v:hashlib.sha256(Path(prefix+v).read_bytes()).hexdigest() for v in ['baseline','candidate']}},indent=2))
 with (r/f'boundary-{mode}.log').open('w') as log:result=subprocess.run(['python3',str(p/'run.py')],stdout=log,stderr=subprocess.STDOUT)
 status[mode]=result.returncode;(r/'boundary-status.json').write_text(json.dumps(status,indent=2));print(mode,result.returncode,flush=True);assert result.returncode==0
