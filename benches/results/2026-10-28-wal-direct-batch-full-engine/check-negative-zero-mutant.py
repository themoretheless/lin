from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent
source=Path('src/persist.rs');before=source.read_bytes()
h=lambda b:hashlib.sha256(b).hexdigest()
helper=(p/'helper.rs').read_text();assert helper.count('x.to_bits() == 0')==1
mutant=helper.replace('x.to_bits() == 0','x.abs().to_bits() == 0')
m=dict(source_before_sha256=h(before),helper_sha256=h(helper.encode()),oracle_sha256=h((p/'oracle.rs').read_bytes()),mutant_helper_sha256=h(mutant.encode()),performance_qualified=False)
try:
 candidate=(p/'candidate-persist.rs').read_text();assert helper in candidate
 source.write_text(candidate.replace(helper,mutant)+'\n'+(p/'oracle.rs').read_text())
 with (p/'negative-zero-mutant.log').open('w') as log:
  r=subprocess.run(['cargo','test','--release','--lib','wal_direct_zero_oracle::legacy_decoder_preserves_complete_records_and_ieee_bits','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode
 s=(p/'negative-zero-mutant.log').read_text()
 m['runtime_mutant_caught']=r.returncode==101 and '1 failed' in s and 'assertion `left == right` failed' in s
 assert m['runtime_mutant_caught'], 'mutant must fail the runtime oracle'
finally:
 source.write_bytes(before);m['source_restored']=source.read_bytes()==before
 (p/'negative-zero-mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps(m,indent=2))
