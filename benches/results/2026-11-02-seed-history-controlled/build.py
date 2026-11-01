from pathlib import Path
import subprocess,json,hashlib,shutil,os,sys
p=Path(__file__).resolve().parent;q=p.parent/'2026-11-01-columnar-batch-wal';sha=lambda b:hashlib.sha256(b).hexdigest();source=Path('src/persist.rs');before=source.read_bytes();status=json.loads((q/'test-status.json').read_text());negative=json.loads((q/'mutant-status.json').read_text());assert before==(q/'candidate-persist.rs').read_bytes()and status['passed']==460 and status['persist_sha256']==sha(before)and negative['source_restored']and negative['runtime_assertion_caught']
others={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source};assert all(sha(subprocess.check_output(['git','show','625bc7180:'+f]))==h for f,h in others.items())
bench=Path('benches/compare.rs');old=bench.read_bytes();target=Path(os.environ['CARGO_TARGET_DIR']).resolve();sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 exe=Path(bench_executable((p/'build.log').read_text(),'compare')).resolve();assert exe.is_relative_to(target);dest=target/'seed-history-controlled';shutil.copy2(exe,dest)
finally:bench.write_bytes(old)
assert source.read_bytes()==before and bench.read_bytes()==old and others=={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if f!=source}
(p/'provenance.json').write_text(json.dumps(dict(binary=str(dest),binary_sha256=sha(dest.read_bytes()),candidate_persist_sha256=sha(before),other_source_sha256=others,harness_sha256=sha((p/'compare.rs').read_bytes()),source_restored=True,fsync_candidate_included=False,single_binary_for_all_modes=True),indent=2)+'\n')
