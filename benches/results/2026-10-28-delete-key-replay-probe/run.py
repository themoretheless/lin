from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;example=Path('examples/delete_key_replay_probe.rs');assert not example.exists();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=hashes,probe_sha256=hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest(),performance_qualified=False)
try:
 example.write_bytes((p/'probe.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','delete_key_replay_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 executables=[]
 for line in(p/'build.log').read_text().splitlines():
  try:q=json.loads(line)
  except ValueError:continue
  if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='delete_key_replay_probe'and q.get('executable'):executables.append(q['executable'])
 assert len(executables)==1;m['binary_sha256']=hashlib.sha256(Path(executables[0]).read_bytes()).hexdigest()
 with(p/'outcomes.jsonl').open('w')as out,(p/'runtime.log').open('w')as log:r=subprocess.run([executables[0],str(p/'fixtures')],stdout=out,stderr=log)
 m['exit']=r.returncode;assert r.returncode==0
finally:
 example.unlink(missing_ok=True);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['all_sources_restored']
