from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text());sys.path.insert(0,str(root/'scripts'));from cargo_bench_artifact import bench_executable
assert not(p/'provenance.json').exists(),'preserve prior attempt'
q=json.loads((p.parent/'2026-11-02-rollback-row-ownership/qualification.json').read_text());assert q['source_restored']and len(q['compiled_negatives'])==3
assert [(s['passed'],s['failed'],s['ignored'])for s in q['suites']]==[(465,0,2),(485,0,4)]
assert q['compiler']==subprocess.check_output(['rustc','-Vv'],text=True)
for n,h in q['source_sha256'].items():assert sha((root/n).read_bytes())==h,n
for n,h in m['build_files'].items():assert sha((root/n).read_bytes())==h,n
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
assert not(root/'.cargo').exists(),'unexpected root Cargo configuration'
assert sha((p/'compare.rs').read_bytes())==m['harness_sha256']
before={str(f.relative_to(root)):f.read_bytes()for f in(root/'src').rglob('*')if f.is_file()};assert {n:sha(b)for n,b in before.items()}==m['roles']['candidate']
bench=root/'benches/compare.rs';oldbench=bench.read_bytes();sources={}
for role in ['baseline','candidate']:
 sources[role]={n:subprocess.check_output(['git','show',m[role]+':'+n],cwd=root)for n in m['roles'][role]};assert {n:sha(b)for n,b in sources[role].items()}==m['roles'][role]
assert sources['baseline'].keys()==sources['candidate'].keys()
target=Path('/tmp/lin-rollback-fts-delta-target-20261102').resolve();env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2');metadata=dict(binaries={},compiler=q['compiler'],features=m['features'],compiled_compare_sha256=m['harness_sha256'],original_compare_sha256=sha(oldbench),instrumentation=False,source_restored=False,commands=[],source_sha256=m['roles']['candidate'],peer_binary_source='baseline')
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  for n,b in sources[role].items():(root/n).write_bytes(b)
  assert {n:sha((root/n).read_bytes())for n in before}==m['roles'][role]
  command=['cargo','bench','--bench','compare','--features','experimental-extent-wal','--no-run','--color','never'];metadata['commands'].append(dict(role=role,command=command));log=p/('build-'+role+'.log');assert not log.exists()
  with log.open('w')as f:r=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  exe=Path(bench_executable(log.read_text(),'compare')).resolve();assert exe.is_relative_to(target)
  dest=Path('/tmp/lin-rollback-row-ownership-'+('base'if role=='baseline'else'cand'));assert not dest.exists();shutil.copy2(exe,dest)
  metadata['binaries'][role]=dict(path=str(dest),sha256=sha(dest.read_bytes()),source_sha256=m['roles'][role],cargo_target_dir=str(target),source_commit=m[role])
finally:
 for n,b in before.items():(root/n).write_bytes(b)
 bench.write_bytes(oldbench)
 metadata['source_restored']=all((root/n).read_bytes()==b for n,b in before.items())and bench.read_bytes()==oldbench and all(sha((root/n).read_bytes())==h for n,h in q['source_sha256'].items())
 (p/'provenance.json').write_text(json.dumps(metadata,indent=2)+'\n')
assert metadata['source_restored']and len(metadata['binaries'])==2
assert metadata['binaries']['baseline']['sha256']!=metadata['binaries']['candidate']['sha256']
print('Uninstrumented paired extent-feature comparison built; all source and harness bytes restored')
