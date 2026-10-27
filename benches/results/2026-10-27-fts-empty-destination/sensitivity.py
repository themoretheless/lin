from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','fts'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};status=[]
try:
 for mode in ['missing_destination_add','missing_source_remove']:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
  f=Path('src/fts.rs');s=f.read_text();a=s.index('    pub(crate) fn move_to_empty_slot(');b=s.index('    /// Re-point one row',a);part=s[a:b]
  needle='self.note_add_to_empty_slot(&tok, to);'if mode=='missing_destination_add'else'self.note_del(&tok, from);';assert part.count(needle)==1;part=part.replace(needle,'// intentional omitted edit',1);f.write_text(s[:a]+part+s[b:]+'\n'+(p/'oracle.rs').read_text())
  with(p/f'mutant-{mode}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','empty_destination_oracle'],stdout=log,stderr=subprocess.STDOUT)
  status.append({'variant':mode,'exit_code':r.returncode});assert r.returncode==101
  log=(p/f'mutant-{mode}.log').read_text();assert 'test result: FAILED.'in log and 'assertion'in log
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 (p/'sensitivity-status.json').write_text(json.dumps(status,indent=2)+'\n')
