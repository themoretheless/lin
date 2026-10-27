from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;files=[Path('src')/(n+'.rs') for n in ['exec','store','check','index']];before={f:f.read_bytes() for f in files};example=Path('examples/cow_vnext_warm.rs');assert not example.exists();h=lambda b:hashlib.sha256(b).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),baseline_commit='6812cc5ca9fc35a4adc81e0948df45dfe3629742',source_sha256={str(f):h(f.read_bytes()) for f in Path('src').rglob('*.rs')},gate_sha256=h((p/'gate.rs').read_bytes()),binaries={},performance_qualified=False)
tests=json.loads((p/'test-status.json').read_text());assert tests['source_sha256']==m['source_sha256'] and tests['exit_code']==0
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for role in ['baseline','candidate']:
  for f in files:f.write_bytes(subprocess.check_output(['git','show',m['baseline_commit']+':'+str(f)]) if role=='baseline' else before[f])
  with(p/('build-'+role+'.log')).open('w') as log:r=subprocess.run(['cargo','build','--release','--example','cow_vnext_warm','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  artifacts=[]
  for line in(p/('build-'+role+'.log')).read_text().splitlines():
   try:d=json.loads(line)
   except ValueError:continue
   if d.get('reason')=='compiler-artifact' and d.get('target',{}).get('name')=='cow_vnext_warm' and d.get('executable'):artifacts.append(d['executable'])
  assert len(artifacts)==1;dest=Path('/tmp/lin-cow-vnext-warm-'+role);shutil.copy2(artifacts[0],dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest.read_bytes()),candidate_sources_sha256={str(f):h(f.read_bytes()) for f in files})
finally:
 for f,b in before.items():f.write_bytes(b)
 example.unlink(missing_ok=True);m['source_restored']=all(h(Path(f).read_bytes())==d for f,d in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
