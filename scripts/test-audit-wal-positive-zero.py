"""SYNTHETIC ONLY: independent WAL source, golden and matrix auditor sensitivity."""
from pathlib import Path
import importlib.util,json,tempfile,shutil,subprocess,tarfile
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('audit',root/'scripts/audit-wal-positive-zero.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
with tempfile.TemporaryDirectory(prefix='lin-wal-zero-independent-synthetic-')as td:
 p=Path(td);(p/'analyze.py').write_bytes(a.git(a.INPUT+'/analyze.py'));(p/'provenance.json').write_text(json.dumps(dict(binaries={r:dict(sha256=r)for r in ['baseline','candidate']})));records=[]
 for repeat in range(1,13):
  for mode in ['baseline','control','candidate']:
   for case in a.CASES:
    value=90 if case in a.CASES[:5]and mode=='candidate'else 100
    q=dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=value,availability=dict(state='available'))for _ in range(8)])
    f=p/f'{repeat:02}-{mode}'/case.replace('/','_')/'run.json';f.parent.mkdir(parents=True,exist_ok=True);f.write_text(json.dumps(q));records.append(dict(round=repeat,mode=mode,case=case,exit_code=0,binary_sha256='baseline'if mode=='control'else mode,elapsed_seconds=1))
 status=p/'process-status.json';status.write_text(json.dumps(records));subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True);assert a.matrix(p)['component_gate_passes']
 def alter(f,change):
  value=json.loads(f.read_text());change(value);f.write_text(json.dumps(value))
 def reject(call,label,f,change):
  before=f.read_bytes();change(f)
  try:call()
  except(AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(label+' accepted')
  f.write_bytes(before)
 check=lambda:a.matrix(p)
 f=p/'01-candidate'/a.CASES[0].replace('/','_')/'run.json'
 reject(check,'missing process',f,lambda x:x.unlink())
 reject(check,'negative observation',f,lambda x:alter(x,lambda q:q['observations'][0].update(value=-1)))
 reject(check,'unavailable observation',f,lambda x:alter(x,lambda q:q['observations'][0]['availability'].update(state='unavailable')))
 reject(check,'wrong metric',f,lambda x:alter(x,lambda q:q['observations'][0].update(metric='cpu')))
 reject(check,'duplicate coordinate',status,lambda x:alter(x,lambda q:q.__setitem__(0,q[1])))
 reject(check,'negative duration',status,lambda x:alter(x,lambda q:q[0].update(elapsed_seconds=-1)))
 reject(check,'wrong binary',status,lambda x:alter(x,lambda q:q[0].update(binary_sha256='candidate')))
 reject(check,'falsified gate',p/'qualification.json',lambda x:alter(x,lambda q:q.update(component_gate_passes=False)))
 q=json.loads(f.read_text())
 for o in q['observations']:o['value']=200
 f.write_text(json.dumps(q));subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,check=True);assert a.matrix(p)['failed_target_windows']==1
 raw=p/'wal-positive-zero-component';raw.mkdir()
 for child in list(p.iterdir()):
  if child!=raw:shutil.move(str(child),str(raw/child.name))
 frozen=a.git(a.INPUT+'/frozen-inputs.json');(raw/'frozen-inputs.json').write_bytes(frozen)
 for name in json.loads(frozen):
  if name.startswith(a.INPUT+'/'):
   dest=raw/name[len(a.INPUT)+1:];dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(a.git(name))
 manifest=json.loads((raw/'input.json').read_text());(p/'run-metadata.json').write_text(json.dumps(dict(headSha=a.HEAD,status='completed',conclusion='failure')))
 (raw/'test-status.json').write_text(json.dumps(dict(exit_code=0,source_sha256=manifest['source_sha256'],release_passed=457,release_ignored=2)))
 (raw/'release-tests.log').write_text('SYNTHETIC ONLY\ntest result: ok. 457 passed; 0 failed; 2 ignored;\n')
 prov=dict(source_commit=a.HEAD,source_restored=True,instrumentation=False,source_sha256=manifest['source_sha256'],compare_sha256=a.sha(a.git(a.INPUT+'/compare.rs')),binaries={role:dict(sha256=role,role_source_sha256=a.source_map(manifest[role+'_reference']))for role in ['baseline','candidate']})
 (raw/'provenance.json').write_text(json.dumps(prov))
 for role in ['baseline','candidate']:(raw/f'build-{role}.log').write_text('SYNTHETIC ONLY\nFinished `bench` profile\n')
 smokes=[]
 for role in ['baseline','candidate']:
  for case in a.CASES:
   smokes.append(dict(role=role,case=case,exit=0));dest=raw/'qualification-smoke'/(role+'-'+case.replace('/','_'))/'run.json';dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(json.dumps(dict(status='complete',environment=dict(os='linux'),cases=[dict(id=case)],observations=[dict(case=case,availability=dict(state='available'))])))
 (raw/'qualification-smoke-status.json').write_text(json.dumps(smokes))
 checks=[]
 for label,name,old,new,test in [('negative-zero','src/persist/positive_zero.rs','if self.0.to_bits() == 0 {','if self.0 == 0.0 {','positive_zero_record_shapes_preserve_canonical_bits_and_size'),('canonical-limit','src/persist.rs','buf.len().checked_add(extra)','buf.len().checked_add(0)','positive_zero_canonical_payload_and_envelope_limits_reject_before_write')]:
  value=a.git(name).decode().replace(old,new);(raw/(label+'-mutant.rs')).write_text(value);checks.append(dict(label=label,source_file=name,mutant_sha256=a.sha(value.encode()),runtime_exit=101,compiled_runtime_rejected=True,source_restored=True))
  (raw/(label+'-runtime.log')).write_text('SYNTHETIC ONLY\nFinished `release` profile\nrunning 1 test\ntest persist::positive_zero::tests::'+test+' ... FAILED\ntest result: FAILED. 0 passed; 1 failed;\n')
 (raw/'mutant-status.json').write_text(json.dumps(dict(checks=checks,source_sha256=manifest['source_sha256'],all_sources_restored=True)))
 fixture=json.loads((raw/'fixture-manifest.json').read_text())
 with tarfile.open(raw/'legacy-qualified-fixtures.tar.gz')as tf:
  for member in tf.getmembers():
   for folder in ['qualified-golden-input','generated-fixtures']:
    dest=raw/folder/member.name;dest.parent.mkdir(exist_ok=True);dest.write_bytes(tf.extractfile(member).read())
 (raw/'golden-status.json').write_text(json.dumps(dict(legacy_reference=fixture['legacy_reference'],golden_members_verified=32,production_qualified_cases_verified=8,source_restored=True,build_exit=0,runtime_exit=0)))
 (raw/'golden-outcomes.jsonl').write_bytes(a.git('benches/results/2026-11-01-wal-positive-zero/current-outcomes.jsonl',manifest['candidate_reference']))
 (raw/'environment.json').write_text(json.dumps(dict(platform='Linux synthetic',rustc='rustc 1.96.0 synthetic\nhost: x86_64-unknown-linux-gnu',same_runtime_all_roles=True,memory_measured=False)));(raw/'linux-environment.txt').write_text('SYNTHETIC ONLY\nFSTYPE\n')
 assert a.audit(p)['failed_target_windows']==1
 check=lambda:a.audit(p)
 reject(check,'wrong head',p/'run-metadata.json',lambda x:alter(x,lambda q:q.update(headSha='bad')))
 reject(check,'changed frozen asset',raw/'run.py',lambda x:x.write_text(x.read_text()+'\n'))
 reject(check,'wrong source provenance',raw/'provenance.json',lambda x:alter(x,lambda q:q.update(source_commit='bad')))
 reject(check,'incorrect baseline source',raw/'provenance.json',lambda x:alter(x,lambda q:q['binaries']['baseline']['role_source_sha256'].update({'src/persist.rs':'bad'})))
 reject(check,'missing smoke',raw/'qualification-smoke-status.json',lambda x:alter(x,lambda q:q.pop()))
 reject(check,'wrong test count',raw/'test-status.json',lambda x:alter(x,lambda q:q.update(release_passed=456)))
 reject(check,'mutant compile only',raw/'negative-zero-runtime.log',lambda x:x.write_text('compile error'))
 reject(check,'unrestored source',raw/'mutant-status.json',lambda x:alter(x,lambda q:q.update(all_sources_restored=False)))
 golden=raw/'generated-fixtures'/next(iter(fixture['files_sha256']))
 reject(check,'changed golden frame',golden,lambda x:x.write_bytes(x.read_bytes()+b'x'))
 assert a.audit(p)['audit_complete']
print('SYNTHETIC ONLY: valid losing target retained; eight matrix and nine provenance/golden corruptions rejected')
