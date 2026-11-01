from pathlib import Path
import os,subprocess,hashlib,json,sys,shutil
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');bench=Path('benches/compare.rs');before=source.read_bytes();before_bench=bench.read_bytes();sha=lambda b:hashlib.sha256(b).hexdigest();target=Path(os.environ['CARGO_TARGET_DIR']).resolve();baseline=subprocess.check_output(['git','show','625bc7180:src/persist.rs'])
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
others={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source};assert all(sha(subprocess.check_output(['git','show','625bc7180:'+f]))==h for f,h in others.items())
try:
 source.write_bytes(baseline);bench.write_bytes((p/'compare.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=Path(bench_executable((p/'build.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target);dest=target/'wal-seed-history';shutil.copy2(exe,dest)
finally:source.write_bytes(before);bench.write_bytes(before_bench)
assert source.read_bytes()==before and bench.read_bytes()==before_bench and others=={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source}
(p/'provenance.json').write_text(json.dumps(dict(runtime_commit='625bc7180',baseline_persist_sha256=sha(baseline),other_source_sha256=others,harness_sha256=sha((p/'compare.rs').read_bytes()),binary=str(dest),binary_sha256=sha(dest.read_bytes()),source_restored=True,fsync_candidate_included=False),indent=2)+'\n')
