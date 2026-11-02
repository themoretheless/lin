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
 for n,d in info['build_sha256'].items():
  assert sha(Path(n).read_bytes())==d
  historical=subprocess.check_output(['git','show',info['commit']+':'+n])
  if n=='Cargo.toml':
   feature=b'# Isolated storage experiment; not selected by Db/OpenOpts.\nexperimental-extent-wal = []\n'
   current=Path(n).read_bytes()
   assert current.count(feature)==1 and historical.count(feature)<=1
   assert current.replace(feature,b'')==historical.replace(feature,b''),'Only explicit empty extent feature declaration may differ'
  else:assert d==sha(historical),n
for n,h in m['runtime_and_tests_sha256'].items():assert sha(Path(n).read_bytes())==h,n
if sys.argv[1:]==['--verify-inputs']:print('PASS exact pair source/dependency inputs');raise SystemExit(0)
assert not sys.argv[1:]
qa=json.loads((p/'correctness-prerequisites.json').read_text())
assert qa['baseline']['run_id']==m['correctness_runs']['baseline']and qa['baseline']['source_commit']==m['baseline']['commit']and qa['baseline']['raw_artifact_verified']and qa['baseline']['qualification_passed']
subprocess.run(['python3',str(p/'audit-candidate.py')],check=True)
candidate_qa=json.loads((p/'candidate-correctness.json').read_text());assert candidate_qa['complete']and candidate_qa['sources_and_tests_restored']and candidate_qa['source_commit']==m['candidate']['commit']and candidate_qa['compiled_negative_checks']==5
before={str(f):f.read_bytes()for f in Path('src').rglob('*.rs')};assert {n:sha(b)for n,b in before.items()}==m['restore_source_sha256']
bench=Path('benches/compare.rs');oldbench=bench.read_bytes();harness=(p/'compare.rs').read_bytes()
env=dict(os.environ);env.setdefault('CARGO_TARGET_DIR','target/cow-extent-full-engine');env.setdefault('CARGO_BUILD_JOBS','2')
assert not any(k.startswith('CARGO_PROFILE_')for k in env)and not env.get('RUSTFLAGS')and not env.get('CARGO_ENCODED_RUSTFLAGS')
target=Path(env['CARGO_TARGET_DIR']).resolve()
meta=dict(instrumentation=False,source_sha256=m['restore_source_sha256'],original_compare_sha256=sha(oldbench),compiled_compare_sha256=sha(harness),frozen_commits={r:m[r]['commit']for r in sources},binaries={},source_restored=False,cargo_target_dir=str(target),assignment_protocol=m['assignment_protocol'])
try:
 bench.write_bytes(harness)
 for role in ['baseline','candidate']:
  for f in Path('src').rglob('*.rs'):
   if str(f)not in sources[role]:f.unlink()
  for n,b in sources[role].items():Path(n).write_bytes(b)
  log=p/('build-'+role+'.log')
  with log.open('w')as output:r=subprocess.run(['cargo','bench','--bench','compare',*(['--features','experimental-extent-wal']if role=='candidate'else[]),'--no-run','--color','never'],env=env,stdout=output,stderr=subprocess.STDOUT)
  assert r.returncode==0
  exe=Path(bench_executable(log.read_text(),'compare')).resolve();assert exe.is_relative_to(target)
  dest=Path('/tmp/lin-cow-extent-full-engine-'+role);assert not dest.exists(),'preserve earlier executable'
  shutil.copy2(exe,dest)
  meta['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),cargo_target_dir=str(target),source_sha256=m[role]['source_sha256'])
  if role=='baseline':
   peer=Path('/tmp/lin-cow-extent-full-engine-external');assert not peer.exists();shutil.copy2(dest,peer)
   meta['peer_binary']=dict(path=str(peer),sha256=sha(peer.read_bytes()),source_commit=m['baseline']['commit'],source_sha256=m['baseline']['source_sha256'],compiled_compare_sha256=sha(harness))
finally:
 for f in Path('src').rglob('*.rs'):
  if str(f)not in before:f.unlink()
 for n,b in before.items():Path(n).write_bytes(b)
 bench.write_bytes(oldbench)
 meta['source_restored']={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')}==m['restore_source_sha256']and bench.read_bytes()==oldbench and all(sha(Path(n).read_bytes())==h for n,h in m['runtime_and_tests_sha256'].items())
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']and len(meta['binaries'])==2;validate(meta)
print('PASS exact source pair restored; all external roles bound to invariant executable')
