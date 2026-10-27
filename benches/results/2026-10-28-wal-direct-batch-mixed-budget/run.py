from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;candidate=Path('benches/results/2026-10-28-wal-direct-batch/candidate-persist.rs');f=Path('src/persist.rs');before=f.read_bytes();h=lambda b:hashlib.sha256(b).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),baseline_sha256=h(before),candidate_sha256=h(candidate.read_bytes()),oracle_sha256=h((p/'oracle.rs').read_bytes()),latency_qualified=False)
assert candidate.read_bytes()==subprocess.check_output(['git','show','696b692a7815759837e97d98ea0ee7912f018e82:'+str(candidate)])
try:
 f.write_bytes(candidate.read_bytes()+b'\n'+(p/'oracle.rs').read_bytes())
 with (p/'tests.log').open('w') as log:r=subprocess.run(['cargo','test','--release','--lib','direct_batch_mixed_budget','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0
 assert '2 passed; 0 failed' in (p/'tests.log').read_text()
finally:
 f.write_bytes(before);m['source_restored']=f.read_bytes()==before;(p/'status.json').write_text(json.dumps(m,indent=2)+'\n')
