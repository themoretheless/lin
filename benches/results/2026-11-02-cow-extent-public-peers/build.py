from pathlib import Path
import hashlib,json,os,shutil,subprocess
from source_manifest import manifest
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();inputs=json.loads((p/'runtime-inputs.json').read_text())
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)and not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
for n,h in inputs['runtime_sha256'].items():assert sha(root/n)==h,n
assert not(p/'build-provenance.json').exists(),'preserve prior attempt'
m=dict(runtime_reference=inputs['runtime_reference'],features=inputs['features'],source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),source_sha256=manifest(),rustc=subprocess.check_output(['rustc','-Vv'],text=True))
command=['cargo','build','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics'];m['build_command']=command
with(p/'build.log').open('w')as log:r=subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,env=dict(os.environ,CARGO_TARGET_DIR='target/cow-peer-integrity-v2',CARGO_BUILD_JOBS='2'))
assert r.returncode==0
artifacts=[]
for line in(p/'build.log').read_text().splitlines():
 try:record=json.loads(line)
 except ValueError:continue
 if record.get('reason')=='compiler-artifact'and record.get('target',{}).get('name')=='peer_bench'and record.get('executable'):artifacts.append(record['executable'])
assert len(artifacts)==1;destination=Path('/tmp/lin-cow-extent-peer-built');assert not destination.exists();shutil.copy2(artifacts[0],destination);m['binary']=dict(path=str(destination),sha256=sha(destination));m['source_unchanged']=manifest()==m['source_sha256'];assert m['source_unchanged'];(p/'build-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
