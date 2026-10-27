from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;candidate=Path('benches/results/2026-10-28-wal-direct-batch/candidate-persist.rs');f=Path('src/persist.rs');before=f.read_bytes();h=lambda b:hashlib.sha256(b).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),baseline_sha256=h(before),candidate_sha256=h(candidate.read_bytes()),oracle_sha256=h((p/'oracle.rs').read_bytes()),latency_qualified=False)
assert candidate.read_bytes()==subprocess.check_output(['git','show','696b692a7815759837e97d98ea0ee7912f018e82:'+str(candidate)])
try:
 f.write_bytes(candidate.read_bytes()+b'\n'+(p/'oracle.rs').read_bytes())
 needle=b'.checked_add(savings)';assert f.read_bytes().count(needle)==1;f.write_bytes(f.read_bytes().replace(needle,b'.checked_add(0)'))
 with (p/'mutant.log').open('w') as log:r=subprocess.run(['cargo','test','--release','--lib','direct_batch_mixed_budget','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;s=(p/'mutant.log').read_text();m['runtime_mutant_caught']=r.returncode==101 and '1 passed; 1 failed' in s and 'panicked at' in s;assert m['runtime_mutant_caught']
finally:
 f.write_bytes(before);m['source_restored']=f.read_bytes()==before;(p/'mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
