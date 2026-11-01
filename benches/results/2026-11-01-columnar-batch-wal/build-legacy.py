from pathlib import Path
import subprocess,json,hashlib,tempfile,os
p=Path(__file__).resolve().parent;ref='04749994b9ca7a090476b95401f0f7166d8b870a';snapshot=Path(tempfile.mkdtemp(prefix='lin-column-batch-legacy-'));sha=lambda b:hashlib.sha256(b).hexdigest();sources={}
names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','lin-derive','tools/peer-worker','Cargo.toml','Cargo.lock','benches/compare.rs'],text=True).splitlines()
for name in names:
 b=subprocess.check_output(['git','show',ref+':'+name]);f=snapshot/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(b);sources[name]=sha(b)
example=snapshot/'examples/column_batch_reader.rs';example.parent.mkdir(exist_ok=True);example.write_bytes((p/'legacy-reader.rs').read_bytes());target=Path('/tmp/lin-column-batch-legacy-target-20261101');env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2')
with(p/'legacy-build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--locked','--example','column_batch_reader','--message-format=json-render-diagnostics'],cwd=snapshot,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
found=[]
for line in(p/'legacy-build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='column_batch_reader'and d.get('executable'):found.append(d['executable'])
assert len(found)==1 and Path(found[0]).resolve().is_relative_to(target.resolve())
assert all(sha((snapshot/f).read_bytes())==h for f,h in sources.items())
(p/'legacy-provenance.json').write_text(json.dumps(dict(legacy_reference=ref,snapshot=str(snapshot),target_dir=str(target),source_sha256=sources,driver_sha256=sha(example.read_bytes()),binary=found[0],binary_sha256=sha(Path(found[0]).read_bytes()),sources_verified=True),indent=2)+'\n')
