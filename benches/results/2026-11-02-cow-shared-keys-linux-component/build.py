from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
inputs=json.loads((p/'input.json').read_text())
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
original={str(f.relative_to(root)):f.read_bytes()for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()}
bench=root/'benches/compare.rs';original_bench=bench.read_bytes();harness=(p/'compare.rs').read_bytes()
assert sha(harness)==inputs['harness_sha256']
env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/'target/cow-shared-key-linux'))
meta=dict(source_restored=False,rustc=subprocess.check_output(['rustc','-Vv'],text=True),harness_sha256=sha(harness),performance_qualified=False,roles={})
def install(files):
 for folder in ['src','tests']:
  for f in(root/folder).rglob('*'):
   if f.is_file() and str(f.relative_to(root))not in files:f.unlink()
 for name,data in files.items():
  f=root/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(data)
try:
 for role in ['baseline','candidate']:
  ref=inputs[role]['commit'];names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','tests'],cwd=root,text=True).splitlines()
  files={n:subprocess.check_output(['git','show',ref+':'+n],cwd=root)for n in names}
  assert {n:sha(b)for n,b in files.items()}==inputs[role]['source_and_tests_sha256']
  install(files);bench.write_bytes(harness)
  qa=['cargo','test','--release'];build=['cargo','bench','--bench','compare','--no-run','--message-format=json-render-diagnostics']
  with(p/f'{role}-tests.log').open('w')as log:q=subprocess.run(qa,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert q.returncode==0,role+' correctness failed'
  with(p/f'{role}-build.log').open('w')as log:b=subprocess.run(build,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert b.returncode==0,role+' benchmark build failed'
  artifacts=[]
  for line in(p/f'{role}-build.log').read_text().splitlines():
   try:d=json.loads(line)
   except ValueError:continue
   if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='compare'and d.get('executable'):artifacts.append(d['executable'])
  assert len(artifacts)==1
  destination=Path('/tmp/lin-shared-key-'+('base'if role=='baseline'else'cand'));assert not destination.exists()
  shutil.copy2(artifacts[0],destination)
  identity=dict(path=str(destination),sha256=sha(destination.read_bytes()))
  meta[role]=identity;meta['roles'][role]=dict(commit=ref,source_and_tests_sha256=inputs[role]['source_and_tests_sha256'],qa_command=qa,qa_exit=q.returncode,build_command=build,build_exit=b.returncode,binary=identity)
  assert all((root/n).read_bytes()==data for n,data in files.items())
finally:
 install(original);bench.write_bytes(original_bench)
 meta['source_restored']=all((root/n).read_bytes()==data for n,data in original.items())and bench.read_bytes()==original_bench
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']
assert meta['baseline']['sha256']!=meta['candidate']['sha256']
