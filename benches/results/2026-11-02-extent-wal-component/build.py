from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
inputs=json.loads((p/'input.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
source={str(f.relative_to(root)):sha(f.read_bytes())for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()}
assert source==inputs['source_and_tests_sha256']
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)and not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
env=dict(os.environ,CARGO_TARGET_DIR=str(root/'target/cow-shared-key-linux'),CARGO_BUILD_JOBS='2')
metadata=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),runtime_reference=inputs['runtime_reference'],source_and_tests_sha256=source,rustc=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False,source_restored=False,commands=[])
bench=root/'benches/compare.rs';original=bench.read_bytes()
try:
 for mode,args in [('feature',['--features','experimental-extent-wal']),('default',[])]:
  command=['cargo','test','--release',*args];metadata['commands'].append(command)
  with(p/(mode+'-tests.log')).open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,mode+' correctness failed'
 bench.write_bytes((p/'compare.rs').read_bytes())
 command=['cargo','bench','--bench','compare','--features','experimental-extent-wal','--no-run','--message-format=json-render-diagnostics'];metadata['commands'].append(command)
 with(p/'build.log').open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 artifacts=[]
 for line in(p/'build.log').read_text().splitlines():
  try:d=json.loads(line)
  except ValueError:continue
  if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='compare'and d.get('executable'):artifacts.append(d['executable'])
 assert len(artifacts)==1
 destination=Path('/tmp/lin-extent-component-current');assert not destination.exists();shutil.copy2(artifacts[0],destination)
 metadata['binary']=dict(path=str(destination),sha256=sha(destination.read_bytes()))
finally:
 bench.write_bytes(original)
 metadata['source_restored']=source=={str(f.relative_to(root)):sha(f.read_bytes())for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()}and bench.read_bytes()==original
 (p/'provenance.json').write_text(json.dumps(metadata,indent=2)+'\n')
assert metadata['source_restored']
