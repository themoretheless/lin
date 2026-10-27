from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items());status=[]
try:
 for mutant in ['missing_old_removal','missing_vacant_reverse']:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
  f=Path('src/index.rs');t=f.read_text();a=t.index('    fn insert_key(');b=t.index('\n    pub fn remove_at(',a);body=t[a:b]
  if mutant=='missing_old_removal':
   assert body.count('vec.remove(idx);')==1;body=body.replace('vec.remove(idx);','/* missing old membership removal */')
  else:
   needle='ReverseEntry::Vacant(entry) => { entry.insert(v.key().clone()); }';assert body.count(needle)==1;body=body.replace(needle,'ReverseEntry::Vacant(entry) => { drop(entry); }')
  f.write_text(t[:a]+body+t[b:]+'\n'+(p/'index-oracle.rs').read_text())
  with(p/f'mutant-{mutant}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','reverse_entry_oracle'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/f'mutant-{mutant}.log').read_text();status.append(dict(mutant=mutant,exit_code=r.returncode));(p/'sensitivity-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==101 and 'assertion' in log and 'could not compile'not in log
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
