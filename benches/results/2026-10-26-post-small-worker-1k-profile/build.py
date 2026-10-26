from pathlib import Path
import subprocess,json,hashlib,re,shutil
p=Path(__file__).resolve().parent;meta={};names=['exec','persist','store']
for n in names:assert Path(f'src/{n}.rs').read_bytes()==(p/f'before-{n}.rs').read_bytes()
meta['source_commit']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();meta['before_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')}
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'instrumented-{n}.rs').read_bytes())
 meta['instrumented_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')}
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0
 exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'build.log').read_text())[1];shutil.copy2(exe,'/tmp/lin-post-small-worker-1k-profile-20261026');meta['binary_sha256']=hashlib.sha256(Path('/tmp/lin-post-small-worker-1k-profile-20261026').read_bytes()).hexdigest();meta['rustc']=subprocess.check_output(['rustc','-Vv'],text=True)
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'before-{n}.rs').read_bytes())
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('built and production source restored',flush=True)
