from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent
source={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
checks=[]
for label,name,old,new,test in [
 ('negative-zero','src/persist/positive_zero.rs','if self.0.to_bits() == 0 {','if self.0 == 0.0 {','positive_zero_record_shapes_preserve_canonical_bits_and_size'),
 ('canonical-limit','src/persist.rs','buf.len().checked_add(extra)','buf.len().checked_add(0)','positive_zero_canonical_payload_and_envelope_limits_reject_before_write'),
]:
 path=Path(name);before=path.read_bytes();text=before.decode();assert text.count(old)==1;mutant=text.replace(old,new);(p/(label+'-mutant.rs')).write_text(mutant)
 status=dict(label=label,source_file=name,mutant_sha256=hashlib.sha256(mutant.encode()).hexdigest())
 try:
  path.write_text(mutant)
  with(p/(label+'-runtime.log')).open('w')as log:r=subprocess.run(['cargo','test','--release','--lib',test,'--color','never'],stdout=log,stderr=subprocess.STDOUT)
  text=(p/(label+'-runtime.log')).read_text();status['runtime_exit']=r.returncode;status['compiled_runtime_rejected']=r.returncode==101 and 'running 1 test'in text and 'test result: FAILED. 0 passed; 1 failed;'in text and 'Finished `release` profile'in text
  assert status['compiled_runtime_rejected'],label
 finally:
  path.write_bytes(before);status['source_restored']=path.read_bytes()==before;checks.append(status)
  (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_sha256=source,all_sources_restored=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in source.items()),performance_qualified=False),indent=2)+'\n')
assert len(checks)==2 and all(c['source_restored']for c in checks)
print('2 separately compiled runtime mutants rejected; source restored')
