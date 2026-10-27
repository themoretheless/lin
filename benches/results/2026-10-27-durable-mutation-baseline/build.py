from pathlib import Path
import hashlib,json,shutil,subprocess,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
label='baseline';source='compare.rs'
p=Path(__file__).resolve().parent;b=Path('benches/compare.rs');before=b.read_bytes()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(instrumentation=False,original_compare_path=str(b),source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},original_compare_sha256=hashlib.sha256(before).hexdigest(),compiled_compare_sha256=h(p/source))
try:
 b.write_bytes((p/source).read_bytes())
 with(p/f'build-{label}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0,'build failed; inspect build.log'
 exe=bench_executable((p/f'build-{label}.log').read_text(),'compare');dest=Path('/tmp/lin-durable-mutation-baseline');shutil.copy2(exe,dest);m['binary']=dict(path=str(dest),sha256=h(dest))
finally:
 b.write_bytes(before);m['source_restored']=b.read_bytes()==before and all(h(f)==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
