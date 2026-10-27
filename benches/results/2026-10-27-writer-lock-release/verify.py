from pathlib import Path
import subprocess,json
out=Path(__file__).resolve().parent
p=Path('src/persist.rs');original=p.read_bytes();s=original.decode();statuses=[]
variants=[('fixed',s,True),('missing_release',s.replace('self.release_writer_lock();','// mutant: omitted unlock',1),False),('missing_owner_guard',s.replace('if std::process::id() == self.lock_owner_pid {','if true {',1),False)]
try:
 for name,source,passed in variants:
  p.write_text(source)
  with (out/f'{name}.log').open('w') as log:
   r=subprocess.run(['cargo','test','--lib','writer_lock_inheritance_tests'],stdout=log,stderr=subprocess.STDOUT)
  statuses.append({'variant':name,'exit_code':r.returncode,'expected_success':passed})
  assert (r.returncode==0)==passed,(name,r.returncode)
finally:
 p.write_bytes(original)
 (out/'status.json').write_text(json.dumps(statuses,indent=2)+'\n')
with (out/'full-tests.log').open('w') as log:
 r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
(out/'full-status.json').write_text(json.dumps({'exit_code':r.returncode})+'\n')
assert r.returncode==0
