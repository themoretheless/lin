"""SYNTHETIC ONLY: schema-undo auditor detects falsified qualification records."""
from pathlib import Path
import importlib.util,json,tempfile
root=Path(__file__).resolve().parents[1];spec=importlib.util.spec_from_file_location('audit',root/'scripts/audit-transaction-schema-undo.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
with tempfile.TemporaryDirectory(prefix='lin-schema-undo-audit-synthetic-')as td:
 p=Path(td);raw=p/'transaction-schema-undo-correctness';raw.mkdir()
 frozen=a.blob(a.PREFIX+'frozen-inputs.json');(raw/'frozen-inputs.json').write_bytes(frozen)
 for n in json.loads(frozen):
  if n.startswith(a.PREFIX):(raw/n[len(a.PREFIX):]).write_bytes(a.blob(n))
 m=json.loads(a.blob(a.PREFIX+'input.json'))
 (p/'run-metadata.json').write_text(json.dumps(dict(headSha=a.HEAD,status='completed',conclusion='success')))
 (raw/'qualification-status.json').write_text(json.dumps(dict(source_commit=a.HEAD,sources_restored=True,tests_unchanged=True,performance_qualified=False,**{'test.py':0,'check-mutants.py':0})))
 (raw/'test-status.json').write_text(json.dumps(dict(exit_code=0,source_sha256=m['source_sha256'],tests_sha256=m['tests_sha256'],release_passed=458,release_ignored=2,performance_qualified=False,compiler='rustc 1.96.0 synthetic\nhost: x86_64-unknown-linux-gnu')))
 (raw/'full-release.log').write_text('SYNTHETIC ONLY\ntest result: ok. 458 passed; 0 failed; 2 ignored;\n'+'\n'.join('test '+c[4]+' ... ok'for c in a.CASES))
 text=a.blob('src/exec.rs').decode();checks=[]
 for label,old,new,count,test in a.CASES:
  b=text.replace(old,new).encode();(raw/(label+'-mutant.rs')).write_bytes(b)
  checks.append(dict(label=label,exit_code=101,compiled_runtime_rejected=True,test=test,mutant_sha256=a.sha(b)))
  (raw/(label+'-runtime.log')).write_text('SYNTHETIC ONLY\nFinished `release` profile\nrunning 1 test\ntest '+test+' ... FAILED\ntest result: FAILED. 0 passed; 1 failed;\n')
 (raw/'mutant-status.json').write_text(json.dumps(dict(source_restored=True,checks=checks)))
 assert a.audit(p)['audit_complete']
 def alter(f,change):
  value=json.loads(f.read_text());change(value);f.write_text(json.dumps(value))
 def reject(label,f,change):
  before=f.read_bytes();change(f)
  try:a.audit(p)
  except(AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(label+' accepted')
  f.write_bytes(before)
 reject('wrong head',p/'run-metadata.json',lambda f:alter(f,lambda d:d.update(headSha='wrong')))
 reject('nonterminal run',p/'run-metadata.json',lambda f:alter(f,lambda d:d.update(status='in_progress')))
 reject('changed frozen script',raw/'test.py',lambda f:f.write_bytes(f.read_bytes()+b'\n'))
 reject('unrestored sources',raw/'qualification-status.json',lambda f:alter(f,lambda d:d.update(sources_restored=False)))
 reject('changed test source',raw/'qualification-status.json',lambda f:alter(f,lambda d:d.update(tests_unchanged=False)))
 reject('failed mutant driver',raw/'qualification-status.json',lambda f:alter(f,lambda d:d.update({'check-mutants.py':1})))
 reject('wrong test count',raw/'test-status.json',lambda f:alter(f,lambda d:d.update(release_passed=457)))
 reject('wrong host',raw/'test-status.json',lambda f:alter(f,lambda d:d.update(compiler='rustc 1.96.0 host: aarch64-apple-darwin')))
 reject('missing positive runtime test',raw/'full-release.log',lambda f:f.write_text('test result: ok. 458 passed; 0 failed; 2 ignored;'))
 reject('compile-only negative',raw/'lost-prefix-journal-runtime.log',lambda f:f.write_text('compile error'))
 reject('different compiled mutant',raw/'missing-checkpoint-mutant.rs',lambda f:f.write_bytes(f.read_bytes()+b'\n'))
 reject('mutant not rejected',raw/'mutant-status.json',lambda f:alter(f,lambda d:d['checks'][0].update(compiled_runtime_rejected=False)))
 assert a.audit(p)['audit_complete']
print('SYNTHETIC ONLY: valid qualification accepted; twelve source, scope, compiler and runtime corruptions rejected')
