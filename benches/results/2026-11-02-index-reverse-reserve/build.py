from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();inputs=json.loads((p/'input.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS') and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
assert not(p/'build-metadata.json').exists()
base=Path('/tmp/lin-index-reverse-baseline');assert not base.exists();subprocess.run(['git','worktree','add','--detach',str(base),'b88ebea2e'],cwd=root,check=True)
metadata=dict(rustc=subprocess.check_output(['rustc','-Vv'],text=True),runtime_reference=inputs['runtime_reference'],commands=[],binaries={},performance_qualified=False)
for role,tree in [('base',base),('cand',root)]:
 env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/('target/index-reverse-'+role)))
 for action in ['test','build']:
  cmd=['cargo',action,'--release','-p','lin-peer-worker'];metadata['commands'].append(dict(role=role,command=cmd))
  with(p/(role+'-'+action+'.log')).open('w')as log:r=subprocess.run(cmd,cwd=tree,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,(role,action)
 destination=Path('/tmp/lin-index-reverse-'+role);assert not destination.exists();shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/lin-peer-worker',destination);metadata['binaries'][role]=dict(path=str(destination),sha256=sha(destination.read_bytes()))
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
(p/'build-metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
