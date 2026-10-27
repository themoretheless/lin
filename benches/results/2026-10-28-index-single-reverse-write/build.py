from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent
h=lambda b:hashlib.sha256(b).hexdigest()
source=Path('src/index.rs');before=source.read_bytes();example=Path('examples/index_single_reverse_probe.rs');assert not example.exists()
inputs=json.loads((p/'input.json').read_text());tests=json.loads((p/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==inputs['source_sha256']
assert all(h(Path(f).read_bytes())==v for f,v in inputs['source_sha256'].items())
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=inputs['source_sha256'],gate_sha256=h((p/'gate.rs').read_bytes()),binaries={},instrumentation=False)
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for role in ['baseline','candidate']:
  source.write_bytes((p/(role+'-index.rs')).read_bytes())
  with(p/('build-'+role+'.log')).open('w')as log:r=subprocess.run(['cargo','build','--release','--example','index_single_reverse_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  artifacts=[]
  for line in(p/('build-'+role+'.log')).read_text().splitlines():
   try:d=json.loads(line)
   except ValueError:continue
   if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='index_single_reverse_probe'and d.get('executable'):artifacts.append(d['executable'])
  assert len(artifacts)==1
  dest=Path('/tmp/lin-index-single-reverse-'+role);shutil.copy2(artifacts[0],dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest.read_bytes()),index_sha256=h(source.read_bytes()))
finally:
 source.write_bytes(before);example.unlink(missing_ok=True);m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in inputs['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['source_restored']
