from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};statuses=[]
try:
 for name in ['skip_changed_nonpartial','skip_partial_predicate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
  f=Path('src/store.rs');s=f.read_text()
  if name=='skip_changed_nonpartial':
   a=s.index('            let unaffected = idx.def.pred.is_none()',s.index('pub(crate) fn index_update_row'));b=s.index('\n            if unaffected',a);s=s[:a]+'            let unaffected = idx.def.pred.is_none();'+s[b:]
  else:s=s.replace('let unaffected = idx.def.pred.is_none()\n                && idx.def.fields','let unaffected = idx.def.fields',1)
  f.write_text(s+'\n'+(p/'oracle.rs').read_text())
  with(p/f'mutant-{name}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','selective_index_oracle'],stdout=log,stderr=subprocess.STDOUT)
  statuses.append({'variant':name,'exit_code':r.returncode});assert r.returncode==101
  assert 'test result: FAILED.' in (p/f'mutant-{name}.log').read_text() and 'assertion' in (p/f'mutant-{name}.log').read_text()
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 (p/'sensitivity-status.json').write_text(json.dumps(statuses,indent=2)+'\n')
