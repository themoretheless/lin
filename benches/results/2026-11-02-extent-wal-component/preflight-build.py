from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
original={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*')if f.is_file()};bench=root/'benches/compare.rs';saved=bench.read_bytes()
metadata=dict(local_only=True,performance_qualified=False,source_unchanged=False)
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 command=['cargo','bench','--bench','compare','--features','experimental-extent-wal','--no-run','--message-format=json-render-diagnostics'];metadata['command']=command
 with(p/'preflight-build.log').open('w')as log:r=subprocess.run(command,cwd=root,env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2'),stdout=log,stderr=subprocess.STDOUT)
 metadata['exit_code']=r.returncode;assert r.returncode==0
 artifacts=[]
 for line in(p/'preflight-build.log').read_text().splitlines():
  try:d=json.loads(line)
  except ValueError:continue
  if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='compare'and d.get('executable'):artifacts.append(d['executable'])
 assert len(artifacts)==1;destination=Path('/tmp/lin-extent-component-local');assert not destination.exists();shutil.copy2(artifacts[0],destination)
 metadata['binary']=dict(path=str(destination),sha256=sha(destination.read_bytes()))
finally:
 bench.write_bytes(saved);metadata['source_unchanged']=original=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*')if f.is_file()}and bench.read_bytes()==saved
 (p/'preflight-build.json').write_text(json.dumps(metadata,indent=2)+'\n')
assert metadata['source_unchanged']
