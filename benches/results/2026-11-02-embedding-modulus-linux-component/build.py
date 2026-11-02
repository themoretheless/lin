from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();inputs=json.loads((p/'input.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
qa=json.loads((p/'correctness.json').read_text());assert qa['source_unchanged'] is True
assert len(qa['suites'])==2
for proof,(mode,passed,ignored)in zip(qa['suites'],[('default',463,2),('feature',483,4)]):
 assert proof['mode']==mode and proof['passed']==passed and proof['failed']==0 and proof['ignored']==ignored
 log=(p/(mode+'-full-tests.log')).read_bytes();assert sha(log)==proof['log_sha256']
 import re
 matches=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)
 assert [sum(int(x[i])for x in matches)for i in range(3)]==[passed,0,ignored]
 assert b'embed::tests::hashing_embedding_matches_original_dense_modulo_bits ... ok'in log

assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS') and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
assert not(p/'build-metadata.json').exists()
base=Path('/tmp/lin-embedding-modulus-baseline');assert not base.exists();subprocess.run(['git','worktree','add','--detach',str(base),'60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'],cwd=root,check=True)
metadata=dict(rustc=subprocess.check_output(['rustc','-Vv'],text=True),runtime_reference=inputs['runtime_reference'],commands=[],binaries={},performance_qualified=False,features=['default','experimental-extent-wal'])
for role,tree in [('base',base),('cand',root)]:
 env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/('target/embedding-modulus-'+role)))
 for action in ['build']:
  cmd=['cargo',action,'--release','--example','peer_bench','--features','experimental-extent-wal'];metadata['commands'].append(dict(role=role,command=cmd))
  with(p/(role+'-'+action+'.log')).open('w')as log:r=subprocess.run(cmd,cwd=tree,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,(role,action)
 destination=Path('/tmp/lin-embedding-modulus-'+role);assert not destination.exists();shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/examples/peer_bench',destination);metadata['binaries'][role]=dict(path=str(destination),sha256=sha(destination.read_bytes()))
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
(p/'build-metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
