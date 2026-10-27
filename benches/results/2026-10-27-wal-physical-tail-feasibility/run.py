from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;f=Path('examples/wal_physical_tail_probe.rs');assert not f.exists();h=lambda data:hashlib.sha256(data).hexdigest();m={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(s):h(s.read_bytes())for s in Path('src').rglob('*.rs')},'probe_sha256':h((p/'probe.rs').read_bytes()),'performance_qualified':False}
try:
 f.write_bytes((p/'probe.rs').read_bytes())
 with(p/'run.log').open('w')as log:r=subprocess.run(['cargo','run','--release','--example','wal_physical_tail_probe'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0;text=(p/'run.log').read_text();result=json.loads(text.splitlines()[-1]);(p/'result.json').write_text(json.dumps(result,indent=2)+'\n')
finally:
 f.unlink(missing_ok=True);m['source_restored']=all(h(Path(s).read_bytes())==v for s,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
