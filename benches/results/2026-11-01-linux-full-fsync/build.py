from pathlib import Path
import subprocess,json,hashlib,os,sys,shutil
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');bench=Path('benches/compare.rs');before=source.read_bytes();before_bench=bench.read_bytes();sha=lambda b:hashlib.sha256(b).hexdigest();target=Path(os.environ['CARGO_TARGET_DIR']).resolve()
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
status=json.loads((p/'test-status.json').read_text());assert status['passed']==458 and status['persist_sha256']==sha(before)
base=(p/'baseline-persist.rs').read_bytes();candidate=(p/'candidate-persist.rs').read_bytes();assert before==candidate and base.replace(b'libc::fdatasync(file.as_raw_fd())',b'libc::fsync(file.as_raw_fd())')==candidate
other={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source};assert all(sha(subprocess.check_output(['git','show','625bc7180:'+f]))==h for f,h in other.items())
result=dict(binaries={},source_restored=False,baseline_persist_sha256=sha(base),candidate_persist_sha256=sha(candidate),harness_sha256=sha((p/'compare.rs').read_bytes()),other_source_sha256=other)
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role,data in [('baseline',base),('candidate',candidate),('missing_barrier',candidate.replace(b'unsafe { libc::fsync(file.as_raw_fd()) }',b'0'))]:
  assert role!='missing_barrier' or data!=candidate
  source.write_bytes(data)
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=Path(bench_executable((p/f'build-{role}.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target)
  dest=target/('linux-full-fsync-'+role);shutil.copy2(exe,dest);result['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),persist_sha256=sha(data))
finally:
 source.write_bytes(before);bench.write_bytes(before_bench);result['source_restored']=source.read_bytes()==before and bench.read_bytes()==before_bench and other=={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source};(p/'provenance.json').write_text(json.dumps(result,indent=2)+'\n')
assert result['source_restored']
