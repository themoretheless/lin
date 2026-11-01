from pathlib import Path
import subprocess,os,sys,json,hashlib,shutil
p=Path(__file__).resolve().parent
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
sha=lambda b:hashlib.sha256(b).hexdigest()
source={str(f):sha(f.read_bytes()) for f in Path('src').rglob('*.rs')}
expected={f:sha(subprocess.check_output(['git','show','625bc7180:'+f]))for f in source}
assert source==expected,'Exact previously qualified runtime required'
bench=Path('benches/compare.rs');original=bench.read_bytes();target=Path(os.environ['CARGO_TARGET_DIR']).resolve()
try:
 bench.write_bytes((p/'diagnostic-compare.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=Path(bench_executable((p/'build.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target)
 binary=target/'wal-syscall-diagnostic';shutil.copy2(exe,binary)
finally:bench.write_bytes(original)
assert source=={str(f):sha(f.read_bytes()) for f in Path('src').rglob('*.rs')} and bench.read_bytes()==original
(p/'provenance.json').write_text(json.dumps(dict(runtime_commit='625bc7180',source_sha256=source,harness_sha256=sha((p/'diagnostic-compare.rs').read_bytes()),binary=str(binary),binary_sha256=sha(binary.read_bytes()),source_restored=True),indent=2)+'\n')
