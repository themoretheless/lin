from pathlib import Path
import subprocess,json,hashlib,shutil,os,sys
p=Path(__file__).resolve().parent;bench=Path('benches/compare.rs');before=bench.read_bytes();source=Path('src/persist.rs');sha=lambda b:hashlib.sha256(b).hexdigest();source_hash=sha(source.read_bytes());target=Path(os.environ['CARGO_TARGET_DIR']).resolve()
status=json.loads((p/'test-status.json').read_text());negative=json.loads((p/'mutant-status.json').read_text());assert status['passed']==460 and status['persist_sha256']==source_hash and negative['source_restored']and negative['runtime_assertion_caught']
assert source.read_bytes()==(p/'candidate-persist.rs').read_bytes()
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 with(p/'build-candidate.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=Path(bench_executable((p/'build-candidate.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target);dest=target/'columnar-batch-candidate';shutil.copy2(exe,dest)
finally:bench.write_bytes(before)
assert sha(source.read_bytes())==source_hash and bench.read_bytes()==before
(p/'provenance.json').write_text(json.dumps(dict(binary=str(dest),binary_sha256=sha(dest.read_bytes()),candidate_persist_sha256=source_hash,harness_sha256=sha((p/'compare.rs').read_bytes()),source_restored=True,fsync_candidate_included=False),indent=2)+'\n')
