from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent;files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes() for f in files};h=lambda b:hashlib.sha256(b).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(b) for f,b in before.items()},probe_sha256={n:h((p/n).read_bytes()) for n in ['exec-probe.rs','store-probe.rs']},performance_qualified=False)
try:
 for f in files:f.write_bytes(before[f]+b'\n'+(p/(f.stem+'-probe.rs')).read_bytes())
 with(p/'tests.log').open('w') as log:r=subprocess.run(['cargo','test','--lib','cow_rollback_owners_probe','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0
 s=(p/'tests.log').read_text();assert '1 passed; 0 failed' in s
 m['owners']=[json.JSONDecoder().raw_decode(part)[0] for part in s.split('COW_OWNERS ')[1:]];assert len(m['owners'])==2
finally:
 for f,b in before.items():f.write_bytes(b)
 m['source_restored']=all(f.read_bytes()==b for f,b in before.items());(p/'status.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps(m,indent=2))
