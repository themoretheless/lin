"""Synthetic sensitivity tests for the independent SoA raw matrix auditor."""
from pathlib import Path
import importlib.util,json,tempfile,shutil,subprocess
root=Path(__file__).resolve().parents[1];spec=importlib.util.spec_from_file_location('soa_audit',root/'scripts/audit-hashing-batch-parallel.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
a.eligible_host(dict(rust_available_parallelism=2,parallel_target_enabled=True,target_rows=10000,target_dim=768))
try:a.eligible_host(dict(rust_available_parallelism=1,parallel_target_enabled=True,target_rows=10000,target_dim=768))
except AssertionError:pass
else:raise RuntimeError('single CPU admitted as parallel target')
with tempfile.TemporaryDirectory(prefix='lin-hashing-independent-synthetic-')as td:
 p=Path(td);(p/'analyze.py').write_bytes(a.git(a.INPUT+'/analyze.py'));(p/'provenance.json').write_text(json.dumps(dict(binaries={role:dict(sha256=role)for role in ['baseline','candidate']})));records=[]
 for repeat in range(1,13):
  for mode in ['baseline','control','candidate']:
   for case in a.CASES:
    target=case=='compare/insert_bulk_10k/lin';value=90 if target and mode=='candidate'else 100
    q=dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=value)for _ in range(8)])
    f=p/f'{repeat:02}-{mode}'/case.replace('/','_')/'run.json';f.parent.mkdir(parents=True,exist_ok=True);f.write_text(json.dumps(q));resource=b'SYNTHETIC ONLY\nUser time (seconds): 0.1\nSystem time (seconds): 0.1\nMaximum resident set size (kbytes): 100\nExit status: 0\n';f.parent.with_suffix('.resources.txt').write_bytes(resource);records.append(dict(round=repeat,mode=mode,case=case,exit_code=0,binary_sha256='baseline'if mode=='control'else mode,elapsed_seconds=1,resource_sha256=a.sha(resource)))
 status=p/'process-status.json';status.write_text(json.dumps(records));subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True);assert a.matrix(p)['component_gate_passes']
 def reject(name,f,change):
  original=f.read_bytes();change(f)
  try:a.matrix(p)
  except(AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(name+' accepted')
  f.write_bytes(original)
 def alter(f,change):
  q=json.loads(f.read_text());change(q);f.write_text(json.dumps(q))
 f=p/'01-candidate'/a.CASES[0].replace('/','_')/'run.json'
 reject('missing process',f,lambda f:f.unlink())
 reject('missing resource record',f.parent.with_suffix('.resources.txt'),lambda f:f.unlink())
 reject('negative observation',f,lambda f:alter(f,lambda q:q['observations'][0].update(value=-1)))
 reject('wrong metric',f,lambda f:alter(f,lambda q:q['observations'][0].update(metric='cpu')))
 reject('duplicate process coordinate',status,lambda f:alter(f,lambda q:q.__setitem__(0,q[1])))
 reject('negative duration',status,lambda f:alter(f,lambda q:q[0].update(elapsed_seconds=-1)))
 reject('falsified qualification',p/'qualification.json',lambda f:alter(f,lambda q:q.update(component_gate_passes=False)))
 assert a.matrix(p)['audit_complete']
 # A real losing target is valid evidence, but must fail the gate.
 q=json.loads(f.read_text())
 for o in q['observations']:o['value']=200
 f.write_text(json.dumps(q));subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True);result=a.matrix(p);assert not result['component_gate_passes']and result['failed_target_windows']==1
print('SYNTHETIC ONLY: independent 324-process audit passed; seven raw corruptions andsingle-CPUhost rejected; losing target retained and gate failed')
