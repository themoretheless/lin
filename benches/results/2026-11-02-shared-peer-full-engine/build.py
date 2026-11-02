from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys
p=Path(__file__).resolve().parent
m=json.loads((p/'source-inputs.json').read_text());assert m['peer_binary_source']=='baseline';sha=lambda b:hashlib.sha256(b).hexdigest()
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
from assignment import validate
sources={}
for role in ['baseline','candidate']:
 info=m[role];names=subprocess.check_output(['git','ls-tree','-r','--name-only',info['commit'],'src'],text=True).splitlines()
 assert {n for n in names if n.endswith('.rs')}==set(info['source_sha256'])
 sources[role]={n:subprocess.check_output(['git','show',info['commit']+':'+n])for n in info['source_sha256']}
 assert {n:sha(b)for n,b in sources[role].items()}==info['source_sha256']
 for n,d in info['build_sha256'].items():assert sha(Path(n).read_bytes())==d==sha(subprocess.check_output(['git','show',info['commit']+':'+n]))
if sys.argv[1:]==['--verify-inputs']:print('PASS exact pair source/dependency inputs');raise SystemExit(0)
assert not sys.argv[1:]
qa=json.loads((p/'correctness-prerequisites.json').read_text())
for role in sources:
 assert qa[role]['run_id']==m['correctness_runs'][role]and qa[role]['source_commit']==m[role]['commit']and qa[role]['raw_artifact_verified']and qa[role]['qualification_passed']
before={str(f):f.read_bytes()for f in Path('src').rglob('*.rs')};assert {n:sha(b)for n,b in before.items()}==m['restore_source_sha256']
bench=Path('benches/compare.rs');oldbench=bench.read_bytes();harness=(p/'compare.rs').read_bytes()
env=dict(os.environ);env.setdefault('CARGO_TARGET_DIR','target/shared-peer-full-engine');env.setdefault('CARGO_BUILD_JOBS','2')
assert not any(k.startswith('CARGO_PROFILE_')for k in env)
target=Path(env['CARGO_TARGET_DIR']).resolve()
meta=dict(instrumentation=False,source_sha256=m['restore_source_sha256'],original_compare_sha256=sha(oldbench),compiled_compare_sha256=sha(harness),frozen_commits={r:m[r]['commit']for r in sources},binaries={},source_restored=False,cargo_target_dir=str(target),assignment_protocol=m['assignment_protocol'])
try:
 bench.write_bytes(harness)
 for role in ['baseline','candidate']:
  for f in Path('src').rglob('*.rs'):
   if str(f)not in sources[role]:f.unlink()
  for n,b in sources[role].items():Path(n).write_bytes(b)
  log=p/('build-'+role+'.log')
  with log.open('w')as output:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],env=env,stdout=output,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=Path(bench_executable(log.read_text(),'compare')).resolve();assert exe.is_relative_to(target)
  dest=Path('/tmp/lin-shared-peer-full-engine-'+role);assert not dest.exists(),'preserve earlier executable'
  shutil.copy2(exe,dest)
  meta['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),cargo_target_dir=str(target),source_sha256=m[role]['source_sha256'])
  if role=='baseline':
   peer=Path('/tmp/lin-shared-peer-full-engine-external');assert not peer.exists();shutil.copy2(dest,peer)
   meta['peer_binary']=dict(path=str(peer),sha256=sha(peer.read_bytes()),source_commit=m['baseline']['commit'],source_sha256=m['baseline']['source_sha256'],compiled_compare_sha256=sha(harness))
finally:
 for f in Path('src').rglob('*.rs'):
  if str(f)not in before:f.unlink()
 for n,b in before.items():Path(n).write_bytes(b)
 bench.write_bytes(oldbench)
 meta['source_restored']={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')}==m['restore_source_sha256']and bench.read_bytes()==oldbench
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']and len(meta['binaries'])==2;validate(meta)
print('PASS exact source pair restored; all external roles bound to invariant executable')
