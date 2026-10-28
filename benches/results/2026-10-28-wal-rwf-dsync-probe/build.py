from pathlib import Path
import subprocess,json,hashlib,shutil,platform
p=Path(__file__).resolve().parent;example=Path('examples/wal_rwf_dsync_probe.rs');assert not example.exists()
m=json.loads((p/'source-input.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==d for f,d in m['source_sha256'].items())
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=m['source_sha256'],gate_sha256=hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),platform=platform.platform(),production_source_changed=False,linux_candidate_smoke_complete=False,performance_qualified=False)
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 with(p/'unit-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--example','wal_rwf_dsync_probe','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 meta['unit_test_exit']=r.returncode;assert r.returncode==0
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','wal_rwf_dsync_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 binaries=[]
 for line in(p/'build.log').read_text().splitlines():
  try:d=json.loads(line)
  except ValueError:continue
  if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='wal_rwf_dsync_probe'and d.get('executable'):binaries.append(d['executable'])
 assert len(binaries)==1;binary=Path('/tmp/lin-wal-rwf-dsync-probe');shutil.copy2(binaries[0],binary);meta['binary']=dict(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
finally:
 example.unlink(missing_ok=True);meta['source_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==d for f,d in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']
