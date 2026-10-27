from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};example=Path('examples/incremental_rollback_gate.rs');assert not example.exists();h=lambda b:hashlib.sha256(b).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')},gate_sha256=h((p/'gate.rs').read_bytes()),binaries={},performance_qualified=False)
functional=json.loads((p.parent/'2026-10-27-incremental-owned-rollback'/'test-status.json').read_text());assert functional['exit_code']==0 and functional['source_restored'] and functional['source_sha256']==m['source_sha256']
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for mode in ['baseline','candidate']:
  for f in files:f.write_bytes(before[f]if mode=='baseline'else((p.parent/'2026-10-27-incremental-owned-rollback')/('candidate-'+f.name)).read_bytes())
  with(p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','incremental_rollback_gate','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,mode;artifacts=[]
  for line in(p/f'build-{mode}.log').read_text().splitlines():
   try:record=json.loads(line)
   except ValueError:continue
   if record.get('reason')=='compiler-artifact'and record.get('target',{}).get('name')=='incremental_rollback_gate'and record.get('executable'):artifacts.append(record['executable'])
  assert len(artifacts)==1;dest=Path('/tmp/lin-incremental-rollback-'+mode);shutil.copy2(artifacts[0],dest);m['binaries'][mode]=dict(path=str(dest),sha256=h(dest.read_bytes()),candidate_sources_sha256={str(f):h(f.read_bytes())for f in files})
finally:
 for f,data in before.items():f.write_bytes(data)
 example.unlink(missing_ok=True);m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
