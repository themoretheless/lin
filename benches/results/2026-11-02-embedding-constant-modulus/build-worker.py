from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();sys.path.insert(0,str(root/'scripts'));from benchmark_source_integrity import snapshot
assert not(p/'worker-build.json').exists(),'preserve prior attempt'
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
archive=root/'benches/results/2026-11-02-cow-extent-public-peers/local-worker-functional.tar.gz'
with tarfile.open(archive)as tar:
 baseline=json.load(tar.extractfile('local-worker-preflight.json'));run=json.load(tar.extractfile('local-worker-native/run.json'))
assert baseline['features']==['default','experimental-extent-wal']and baseline['source_unchanged']and run['source_identity_before']==run['source_identity_after'];before=snapshot(root,None)['files_sha256'];old=run['source_identity_before']['files_sha256'];assert set(before)==set(old);assert {n for n in before if before[n]!=old[n]}=={'src/embed.rs'}
base=Path(baseline['binary']['path']);assert sha(base.read_bytes())==baseline['binary']['sha256']==run['lin_binary_info']['sha256'];compiler=subprocess.check_output(['rustc','-Vv'],text=True);assert compiler==baseline['compiler']
for mode,passed,ignored in [('default',463,2),('feature',483,4)]:
 proof=json.loads((p/(mode+'-status.json')).read_text());assert proof['passed']==passed and proof['failed']==0 and proof['ignored']==ignored and proof['log_sha256']==sha((p/(mode+'-tests.log')).read_bytes())
command=['cargo','build','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics'];assert command==baseline['commands'][0]
with(p/'worker-build.log').open('w')as log:r=subprocess.run(command,cwd=root,env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2'),stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
artifacts=[]
for line in(p/'worker-build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='peer_bench'and d.get('executable'):artifacts.append(d['executable'])
assert len(artifacts)==1;binary=Path('/tmp/lin-embedding-constant-modulus-public-peer-local');assert not binary.exists();shutil.copy2(artifacts[0],binary);assert before==snapshot(root,None)['files_sha256']
(p/'worker-build.json').write_text(json.dumps(dict(compiler=compiler,features=baseline['features'],command=command,baseline=dict(path=str(base),sha256=sha(base.read_bytes()),source_sha256=old),candidate=dict(path=str(binary),sha256=sha(binary.read_bytes()),source_sha256=before),performance_qualified=False),indent=2)+'\n');print('Paired exact-feature workers ready; only embedding source differs')
