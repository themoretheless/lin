from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/exec.rs');original=source.read_bytes();text=original.decode();checks=[]
old='if k == "hash" && !explicit_hash {'
assert text.count(old)==1
try:
 for label,new in [('guard-derived-hash','if false {'),('skip-explicit-hash-guard','if k == "hash" {')]:
  mutant=text.replace(old,new).encode();source.write_bytes(mutant);(p/(label+'-mutant.rs')).write_bytes(mutant)
  with(p/(label+'-runtime.log')).open('w')as log:
   r=subprocess.run(['cargo','test','--release','--test','update_patch_guard','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/(label+'-runtime.log')).read_text()
  rejected=r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed;'in log and 'Finished `release` profile'in log and 'test update_patch_preserves_explicit_and_derived_hash_guards ... FAILED'in log
  checks.append(dict(label=label,exit_code=r.returncode,compiled_runtime_rejected=rejected,mutant_sha256=hashlib.sha256(mutant).hexdigest()))
  assert rejected,label
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_restored=source.read_bytes()==original),indent=2)+'\n')
assert len(checks)==2
