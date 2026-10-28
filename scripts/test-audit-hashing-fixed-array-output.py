"""Synthetic sensitivity tests for the independent sparse hashing output raw matrix auditor."""
from pathlib import Path
import importlib.util,json,tempfile,shutil,subprocess
root=Path(__file__).resolve().parents[1];spec=importlib.util.spec_from_file_location('soa_audit',root/'scripts/audit-hashing-fixed-array-output.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
a.eligible_host(dict(rust_available_parallelism=1,candidate_fixed_array_output=True))
try:a.eligible_host(dict(rust_available_parallelism=0,candidate_fixed_array_output=True))
except AssertionError:pass
else:raise RuntimeError('zero-CPU host metadata admitted')
baseline=a.git('src/embed.rs','a7d4b02c3').decode();candidate=a.git('src/embed.rs').decode();a.fixed_only(baseline,candidate)
try:a.fixed_only(baseline,candidate.replace('Arc::new([0.0f32; 768])','Arc::new([1.0f32; 768])'))
except AssertionError:pass
else:raise RuntimeError('extra output copy change accepted')
with tempfile.TemporaryDirectory(prefix='lin-serial-bulk-independent-synthetic-')as td:
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
 # End-to-end synthetic artifact: metadata/logs below are fixtures, never measurements.
 raw=p/'hashing-fixed-array-output-component';raw.mkdir()
 for child in list(p.iterdir()):
  if child!=raw:shutil.move(str(child),str(raw/child.name))
 frozen=a.git(a.INPUT+'/frozen-inputs.json');(raw/'frozen-inputs.json').write_bytes(frozen)
 for name in json.loads(frozen):
  if name.startswith(a.INPUT+'/'):
   dest=raw/name[len(a.INPUT)+1:];dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(a.git(name))
 manifest=json.loads((raw/'input.json').read_text())
 (p/'run-metadata.json').write_text(json.dumps(dict(head_sha=a.HEAD,status='completed',conclusion='failure')))
 (raw/'test-status.json').write_text(json.dumps(dict(exit_code=0,source_sha256=manifest['source_sha256'])))
 (raw/'release-tests.log').write_text('SYNTHETIC ONLY\ntest result: ok. 450 passed; 0 failed; 2 ignored;\n')
 prov=dict(source_commit=a.HEAD,source_restored=True,instrumentation=False,source_sha256=manifest['source_sha256'],compare_sha256=a.sha(a.git(a.INPUT+'/compare.rs')),binaries={role:dict(sha256=role,embed_sha256=a.sha((raw/f'{role}-embed.rs').read_bytes()))for role in ['baseline','candidate']})
 (raw/'provenance.json').write_text(json.dumps(prov));smokes=[]
 for role in ['baseline','candidate']:
  for case in a.CASES:
   smokes.append(dict(role=role,case=case,exit=0));f=raw/'smoke'/(role+'-'+case.replace('/','_'))/'run.json';f.parent.mkdir(parents=True);f.write_text(json.dumps(dict(status='complete',cases=[dict(id=case)],observations=[dict(synthetic=True)])))
 (raw/'smoke-status.json').write_text(json.dumps(smokes));(raw/'host.json').write_text(json.dumps(dict(rust_available_parallelism=1,candidate_fixed_array_output=True)))
 original=a.git('src/embed.rs');mutant=original.replace(b'            for &i in &self.touched {\n                slots[i as usize] = self.v[i as usize];\n            }',b'            for &i in self.touched.iter().skip(1) {\n                slots[i as usize] = self.v[i as usize];\n            }')
 (raw/'mutant-embed.rs').write_bytes(mutant);(raw/'mutant-status.json').write_text(json.dumps(dict(runtime_mutant_rejected=True,all_sources_restored=True,exit=101,original_source_sha256=manifest['source_sha256'],mutant_sha256=a.sha(mutant))))
 (raw/'mutant-runtime.log').write_text('SYNTHETIC ONLY\npanicked\ntest result: FAILED. 0 passed; 1 failed;\n');(raw/'linux-environment.txt').write_text('SYNTHETIC ONLY FSTYPE\n')
 result=a.audit(p);assert result['audit_complete']and not result['component_gate_passes']and result['fixed_array_output_only']
 def reject_full(name,f,change):
  original=f.read_bytes();change(f)
  try:a.audit(p)
  except(AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(name+' accepted in complete artifact')
  f.write_bytes(original)
 reject_full('wrong source revision',p/'run-metadata.json',lambda f:alter(f,lambda q:q.update(head_sha='wrong')))
 reject_full('changed frozen asset',raw/'run.py',lambda f:f.write_bytes(f.read_bytes()+b'\n# unexpected'))
 reject_full('missing release coverage',raw/'release-tests.log',lambda f:f.write_text('test result: ok. 449 passed; 0 failed; 2 ignored;'))
 reject_full('unrestored mutation',raw/'mutant-status.json',lambda f:alter(f,lambda q:q.update(all_sources_restored=False)))
 reject_full('invalid mutant source',raw/'mutant-embed.rs',lambda f:f.write_bytes(original))
 reject_full('incomplete correctness smokes',raw/'smoke-status.json',lambda f:alter(f,lambda q:q.pop()))
 assert a.audit(p)['audit_complete']
print('SYNTHETIC ONLY: independent 324-process and full-artifact audits passed; seven matrix and six provenance corruptions rejected; losing target retained and gate failed')
