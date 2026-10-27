from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;proto=p.parent/'2026-10-27-cow-bitmap-empty-check';names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};example=Path('examples/mutation_allocation_gate.rs');assert not example.exists();meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),'variant_source_sha256':{}}
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for mode in ['baseline','candidate']:
  meta['variant_source_sha256'][mode]={}
  for n in names:
   b=(proto/f'{"baseline-cow"if mode=="baseline"else"candidate"}-{n}.rs').read_bytes();Path(f'src/{n}.rs').write_bytes(b);meta['variant_source_sha256'][mode][n]=hashlib.sha256(b).hexdigest()
  with(p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','mutation_allocation_gate'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  binary=Path(f'/tmp/lin-cow-bitmap-empty-warm-{mode}');shutil.copy2('target/release/examples/mutation_allocation_gate',binary);meta[mode+'_binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
