from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/"scripts"))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['fts','store'];before={n:Path(f'src/{n}.rs').read_text()for n in names};meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}}
for n,s in before.items():(p/f'before-{n}.rs').write_text(s)
candidate={n:(p/f'candidate-{n}.rs').read_text() for n in names}
try:
 for mode,source in [('baseline',before),('candidate',candidate)]:
  for n,s in source.items():Path(f'src/{n}.rs').write_text(s)
  with (p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,mode
  exe=bench_executable((p/f'build-{mode}.log').read_text(),'compare');dest=f'/tmp/lin-fts-swap-delete-{mode}';shutil.copy2(exe,dest);meta[mode+'_binary_sha256']=hashlib.sha256(Path(dest).read_bytes()).hexdigest()
finally:
 for n,s in before.items():Path(f'src/{n}.rs').write_text(s)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('both binaries built; source restored',flush=True)
