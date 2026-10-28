"""SYNTHETIC ONLY: test matrix and frozen-artifact auditor failure sensitivity."""
from pathlib import Path
import importlib.util,json,tempfile,shutil,subprocess
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('audit',root/'scripts/audit-rollback-fts-sync.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
with tempfile.TemporaryDirectory(prefix='lin-fts-independent-synthetic-')as td:
 p=Path(td);(p/'analyze.py').write_bytes(a.git(a.INPUT+'/analyze.py'))
 (p/'provenance.json').write_text(json.dumps(dict(binaries={role:dict(sha256=role)for role in ['baseline','candidate']})));records=[]
 for repeat in range(1,13):
  for mode in ['baseline','control','candidate']:
   for case in a.CASES:
    target=case.startswith('compare/durable_rollback_1row_')and case.endswith('/lin');value=90 if target and mode=='candidate'else 100
    q=dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=value,availability=dict(state='available'))for _ in range(8)])
    f=p/f'{repeat:02}-{mode}'/case.replace('/','_')/'run.json';f.parent.mkdir(parents=True,exist_ok=True);f.write_text(json.dumps(q))
    records.append(dict(round=repeat,mode=mode,case=case,exit_code=0,binary_sha256='baseline'if mode=='control'else mode,elapsed_seconds=1))
 status=p/'process-status.json';status.write_text(json.dumps(records))
 subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True);assert a.matrix(p)['component_gate_passes']
 def reject(call,name,f,change):
  original=f.read_bytes();change(f)
  try:call()
  except(AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(name+' accepted')
  f.write_bytes(original)
 def alter(f,change):
  q=json.loads(f.read_text());change(q);f.write_text(json.dumps(q))
 check=lambda:a.matrix(p)
 f=p/'01-candidate'/a.CASES[0].replace('/','_')/'run.json'
 reject(check,'missing process',f,lambda f:f.unlink())
 reject(check,'negative observation',f,lambda f:alter(f,lambda q:q['observations'][0].update(value=-1)))
 reject(check,'unavailable observation',f,lambda f:alter(f,lambda q:q['observations'][0]['availability'].update(state='unavailable')))
 reject(check,'wrong metric',f,lambda f:alter(f,lambda q:q['observations'][0].update(metric='cpu')))
 reject(check,'duplicate coordinate',status,lambda f:alter(f,lambda q:q.__setitem__(0,q[1])))
 reject(check,'negative duration',status,lambda f:alter(f,lambda q:q[0].update(elapsed_seconds=-1)))
 reject(check,'wrong binary role',status,lambda f:alter(f,lambda q:q[0].update(binary_sha256='candidate')))
 reject(check,'falsified qualification',p/'qualification.json',lambda f:alter(f,lambda q:q.update(component_gate_passes=False)))
 q=json.loads(f.read_text())
 for o in q['observations']:o['value']=200
 f.write_text(json.dumps(q));subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True)
 result=a.matrix(p);assert not result['component_gate_passes']and result['failed_target_windows']==1
 raw=p/'rollback-fts-sync';raw.mkdir()
 for child in list(p.iterdir()):
  if child!=raw:shutil.move(str(child),str(raw/child.name))
 frozen=a.git(a.INPUT+'/frozen-inputs.json');(raw/'frozen-inputs.json').write_bytes(frozen)
 for name in json.loads(frozen):
  if name.startswith(a.INPUT+'/'):
   dest=raw/name[len(a.INPUT)+1:];dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(a.git(name))
 manifest=json.loads((raw/'input.json').read_text())
 (p/'run-metadata.json').write_text(json.dumps(dict(headSha=a.HEAD,status='completed',conclusion='failure')))
 (raw/'test-status.json').write_text(json.dumps(dict(exit_code=0,source_sha256=manifest['source_sha256'],release_passed=454,release_ignored=2)))
 (raw/'release-tests.log').write_text('SYNTHETIC ONLY\ntest result: ok. 454 passed; 0 failed; 2 ignored;\n')
 prov=dict(source_commit=a.HEAD,source_restored=True,instrumentation=False,source_sha256=manifest['source_sha256'],compare_sha256=a.sha(a.git(a.INPUT+'/compare.rs')),binaries={role:dict(sha256=role,store_sha256=a.sha((raw/f'{role}-store.rs').read_bytes()))for role in ['baseline','candidate']})
 (raw/'provenance.json').write_text(json.dumps(prov))
 for role in ['baseline','candidate']:(raw/f'build-{role}.log').write_text('SYNTHETIC ONLY\nFinished `bench` profile\n')
 smokes=[]
 for role in ['baseline','candidate']:
  for case in a.CASES:
   smokes.append(dict(role=role,case=case,exit=0));dest=raw/'smoke'/(role+'-'+case.replace('/','_'))/'run.json';dest.parent.mkdir(parents=True,exist_ok=True)
   dest.write_text(json.dumps(dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,availability=dict(state='available'))])))
 (raw/'smoke-status.json').write_text(json.dumps(smokes))
 original=(raw/'candidate-store.rs').read_text();needle='            fts.sync_row(idx, &current, &restored);'
 (raw/'mutant-store.rs').write_text(original.replace(needle,'            let _ = fts; // deliberately omit rollback FTS synchronization'))
 (raw/'mutant-status.json').write_text(json.dumps(dict(runtime_exit=101,compiled_runtime_rejected=True,source_restored=True,source_sha256=a.sha(original.encode()))))
 (raw/'mutant-runtime.log').write_text('SYNTHETIC ONLY\nFinished `release` profile\nrunning 1 test\ntest store::inplace_patch_engine_oracle::held_row_isolated_and_body_hash_fts_maps_restore_on_rollback ... FAILED\ntest result: FAILED. 0 passed; 1 failed;\n')
 (raw/'environment.json').write_text(json.dumps(dict(platform='Linux synthetic',rustc='rustc 1.96.0 synthetic\nhost: x86_64-unknown-linux-gnu',same_runtime_all_roles=True,memory_measured=False)))
 (raw/'linux-environment.txt').write_text('SYNTHETIC ONLY\nFSTYPE\n')
 assert a.audit(p)['failed_target_windows']==1
 check=lambda:a.audit(p)
 reject(check,'wrong head',p/'run-metadata.json',lambda f:alter(f,lambda q:q.update(headSha='bad')))
 reject(check,'changed frozen asset',raw/'run.py',lambda f:f.write_text(f.read_text()+'\n'))
 reject(check,'wrong source provenance',raw/'provenance.json',lambda f:alter(f,lambda q:q.update(source_commit='bad')))
 reject(check,'missing smoke',raw/'smoke-status.json',lambda f:alter(f,lambda q:q.pop()))
 reject(check,'wrong test count',raw/'test-status.json',lambda f:alter(f,lambda q:q.update(release_passed=453)))
 reject(check,'mutant compile only',raw/'mutant-runtime.log',lambda f:f.write_text('compile failure'))
 reject(check,'unrestored source',raw/'mutant-status.json',lambda f:alter(f,lambda q:q.update(source_restored=False)))
 assert a.audit(p)['audit_complete']
print('SYNTHETIC ONLY: valid losing-target evidence retained; eight matrix and seven frozen-provenance corruptions rejected')
