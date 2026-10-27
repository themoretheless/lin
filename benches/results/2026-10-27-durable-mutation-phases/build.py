from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;names=['exec','persist','store','lib'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};bench=Path('benches/compare.rs');bench_before=bench.read_bytes();module=Path('src/mutation_profile.rs');assert not module.exists()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},original_compare_sha256=hashlib.sha256(bench_before).hexdigest(),instrumented_source_sha256={n:h(p/f'instrumented-{n}.rs')for n in names},profile_sha256=h(p/'profile.rs'),binaries={},instrumentation='caller-thread scoped clocks; in-memory event buffer; JSON flush after suite; original and disabled controls')
try:
 for role in ['original','instrumented']:
  for n in names:Path(f'src/{n}.rs').write_bytes(before[n]if role=='original'else(p/f'instrumented-{n}.rs').read_bytes())
  if role=='instrumented':module.write_bytes((p/'profile.rs').read_bytes())
  else:module.unlink(missing_ok=True)
  bench.write_bytes((p/f'{role}-compare.rs').read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path(f'/tmp/lin-durable-mutation-phases-{role}');shutil.copy2(exe,dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest))
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 module.unlink(missing_ok=True);bench.write_bytes(bench_before);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items())and bench.read_bytes()==bench_before;(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
