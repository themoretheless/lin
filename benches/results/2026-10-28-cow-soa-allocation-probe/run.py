from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;example=Path('examples/soa_allocation_probe.rs');assert not example.exists()
hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=hashes,gate_sha256=hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),performance_qualified=False)
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','soa_allocation_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 items=[]
 for line in(p/'build.log').read_text().splitlines():
  try:d=json.loads(line)
  except ValueError:continue
  if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='soa_allocation_probe'and d.get('executable'):items.append(d['executable'])
 assert len(items)==1;binary=Path(items[0]);meta['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
 for shape in ['short','long','mixed']:
  for repeat in range(3):
   r=subprocess.run([str(binary),shape],capture_output=True,text=True);(p/f'{shape}-{repeat}.json').write_text(r.stdout);(p/f'{shape}-{repeat}.stderr').write_text(r.stderr);assert r.returncode==0,r.stderr
finally:
 example.unlink(missing_ok=True);meta['production_source_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['production_source_restored']
print('nine isolated allocation diagnostics complete; no latency/RSS/DB performance claim')
