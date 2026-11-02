from pathlib import Path
import hashlib,json,os,shutil,subprocess
from source_manifest import manifest
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert not(p/'local-worker-preflight.json').exists(),'preserve prior attempt'
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
before=manifest();env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2');meta=dict(functional_only=True,performance_qualified=False,external_peer_comparisons=0,compiler=subprocess.check_output(['rustc','-Vv'],text=True),features=['default','experimental-extent-wal'],commands=[],source_unchanged=False)
cmd=['cargo','build','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics'];meta['commands'].append(cmd)
with(p/'local-worker-build.log').open('w')as log:r=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
artifacts=[]
for line in(p/'local-worker-build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='peer_bench'and d.get('executable'):artifacts.append(d['executable'])
assert len(artifacts)==1;binary=Path('/tmp/lin-cow-extent-public-peer-local');assert not binary.exists();shutil.copy2(artifacts[0],binary);meta['binary']=dict(path=str(binary),sha256=sha(binary))
python='/tmp/lin-cow-extent-peer-bindings-20261102/bin/python'
for mode,n,cases in [('read',1000,[]),('native',100000,['--cases','insert_native'])]:
 output=p/('local-worker-'+mode);assert not output.exists();cmd=[python,'scripts/bench-peers.py','--engines','lin',*cases,'--rows',str(n),'--samples','1','--repeats','1','--sample-ms','1','--lin-binary',str(binary),'--output',str(output)];meta['commands'].append(cmd)
 with(p/('local-worker-'+mode+'.log')).open('w')as log:r=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0,mode
 d=json.loads((output/'run.json').read_text());assert d['status']=='complete'and not d['errors']and not d['source_drift']and d['engines']==['lin']and len(d['observations'])==(6 if mode=='read'else 1)
 for o in d['observations']:assert len(o['samples_ns'])==1
 meta[mode]=dict(rows=n,observations=len(d['observations']),full_value_oracle_verified=True,run_json_sha256=sha(output/'run.json'))
assert before==manifest()and sha(binary)==meta['binary']['sha256'];meta['source_unchanged']=True;meta['complete']=True;(p/'local-worker-preflight.json').write_text(json.dumps(meta,indent=2)+'\n');print('PASS six read queries and native100k exact-feature worker; functional only, no external peer comparisons')
