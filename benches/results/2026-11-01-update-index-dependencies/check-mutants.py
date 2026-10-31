from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent
source=Path('src/store.rs');original=source.read_bytes();text=original.decode();checks=[]
old='idx.def.pred.is_some()\n                    || idx.def.fields.iter().any(|field| patch.contains_key(field))'
assert text.count(old)==1
try:
 for label,new in [
  ('skip-partial-predicate','false\n                    || idx.def.fields.iter().any(|field| patch.contains_key(field))'),
  ('skip-changed-key','idx.def.pred.is_some()\n                    || false'),
 ]:
  mutant=text.replace(old,new).encode();source.write_bytes(mutant);(p/(label+'-mutant.rs')).write_bytes(mutant)
  with (p/(label+'-runtime.log')).open('w') as log:
   r=subprocess.run(['cargo','test','--release','--lib','selective_index_patch_matches_full_reindex','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/(label+'-runtime.log')).read_text()
  rejected=r.returncode==101 and 'test result: FAILED. 0 passed; 1 failed;' in log and 'Finished `release` profile' in log
  checks.append(dict(label=label,exit_code=r.returncode,compiled_runtime_rejected=rejected,mutant_sha256=hashlib.sha256(mutant).hexdigest()))
  assert rejected,label
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_restored=source.read_bytes()==original),indent=2)+'\n')
assert len(checks)==2
