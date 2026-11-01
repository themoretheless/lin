from pathlib import Path
import subprocess,json,hashlib,os,sys,shutil
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');bench=Path('benches/compare.rs');before=source.read_bytes();old_bench=bench.read_bytes();sha=lambda b:hashlib.sha256(b).hexdigest();target=Path(os.environ['CARGO_TARGET_DIR']).resolve()
assert before==(p/'candidate-persist.rs').read_bytes()and (p/'baseline-persist.rs').read_bytes()==subprocess.check_output(['git','show','625bc7180:src/persist.rs'])
status=json.loads((p/'test-status.json').read_text());negative=json.loads((p/'mutant-status.json').read_text());assert status['passed']==460 and status['persist_sha256']==sha(before)and negative['source_restored']and negative['runtime_assertion_caught']
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
other={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source}
assert all(sha(subprocess.check_output(['git','show','625bc7180:'+f]))==h for f,h in other.items())
result=dict(binaries={},other_source_sha256=other,harness_sha256=sha((p/'benchmark-compare.rs').read_bytes()),source_restored=False)
try:
 bench.write_bytes((p/'benchmark-compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  data=(p/f'{role}-persist.rs').read_bytes();source.write_bytes(data)
  with(p/f'build-benchmark-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=Path(bench_executable((p/f'build-benchmark-{role}.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target);dest=target/('columnar-batch-benchmark-'+role);shutil.copy2(exe,dest);result['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),persist_sha256=sha(data))
finally:
 source.write_bytes(before);bench.write_bytes(old_bench);result['source_restored']=source.read_bytes()==before and bench.read_bytes()==old_bench;(p/'benchmark-provenance.json').write_text(json.dumps(result,indent=2)+'\n')
assert result['source_restored']and other=={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source}
