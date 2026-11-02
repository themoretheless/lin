from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sys.path.insert(0,str(root/'scripts'));from benchmark_source_integrity import snapshot
sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text())
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
assert not(p/'build-provenance.json').exists();before=snapshot(root,None)['files_sha256'];command=['cargo','build','--locked','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics']
with(p/'build.log').open('w')as f:r=subprocess.run(command,cwd=root,env=dict(os.environ,CARGO_BUILD_JOBS='2'),stdout=f,stderr=subprocess.STDOUT)
assert r.returncode==0
executables=[]
for line in(p/'build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='peer_bench'and d.get('executable'):executables.append(d['executable'])
assert len(executables)==1;dest=Path('/tmp/lin-cow-extent-mssql-peer');assert not dest.exists();shutil.copy2(executables[0],dest)
assert before==snapshot(root,None)['files_sha256'];compiler=subprocess.check_output(['rustc','-Vv'],text=True);assert 'release: 1.96.0'in compiler and 'host: x86_64-unknown-linux-gnu'in compiler
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
(p/'build-provenance.json').write_text(json.dumps(dict(runtime_reference=m['runtime_reference'],source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),compiler=compiler,command=command,features=m['features'],source_sha256=before,binary=dict(path=str(dest),sha256=sha(dest.read_bytes())),source_unchanged=True),indent=2)+'\n')
print('Exact current root worker built and source closure verified')
