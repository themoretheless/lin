from pathlib import Path
import subprocess,json,hashlib,shutil
r=Path(__file__).resolve().parent;names=['exec'];example=Path('examples/small_neon_floor_native.rs');assert not example.exists();meta={}
source=(r/'interleaved-worker.rs').read_text();source=source.replace('let mut db=Db::open(dir.path())?;db.run("index docs [wing, ts]")?;', 'let mut db=Db::empty();db.run("index docs [wing, ts]")?;')
assert source!=(r/'interleaved-worker.rs').read_text()
try:
 example.write_text(source)
 for v in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((r/f'{"before" if v=="baseline" else "candidate"}-{n}.rs').read_bytes())
  with (r/f'native-{v}-build.log').open('w') as log:p=subprocess.run(['cargo','build','--release','--example','small_neon_floor_native'],stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0
  binary='/tmp/lin-small-neon-floor-native-ab-'+v;shutil.copy2('target/release/examples/small_neon_floor_native',binary)
  meta[v]={'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True)}
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((r/f'candidate-{n}.rs').read_bytes())
 example.unlink(missing_ok=True)
(r/'native-provenance.json').write_text(json.dumps(meta,indent=2))
for v in ['baseline','candidate']:shutil.copy2('/tmp/lin-small-neon-floor-native-ab-baseline','/tmp/lin-small-neon-floor-native-aa-'+v)
reference=Path('benches/results/2026-10-24-interleaved-durable-control/run.py').read_text();status={}
for mode in ['aa','ab','aa-s2','ab-s2']:
 base=mode.split('-')[0];p=r/f'native-{mode}';p.mkdir();prefix='/tmp/lin-small-neon-floor-native-'+base+'-';(p/'run.py').write_text(reference.replace('/tmp/lin-interleaved-durable-',prefix))
 (p/'provenance.json').write_text(json.dumps({'source_sha256':meta['candidate']['source_sha256'],'baseline_source_sha256':meta['baseline']['source_sha256'],'binary_sha256':{v:hashlib.sha256(Path(prefix+v).read_bytes()).hexdigest() for v in ['baseline','candidate']}},indent=2))
 with (r/f'native-{mode}.log').open('w') as log:result=subprocess.run(['python3',str(p/'run.py')],stdout=log,stderr=subprocess.STDOUT)
 status[mode]=result.returncode;(r/'native-status.json').write_text(json.dumps(status,indent=2));print(mode,result.returncode,flush=True);assert result.returncode==0
