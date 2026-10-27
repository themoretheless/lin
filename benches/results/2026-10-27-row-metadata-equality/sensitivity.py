from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items());status=[]
try:
 for mutant,needle in [('missing_wing_refresh','self.docs_wing[idx] = text("wing");'),('missing_title_invalidation','self.docs_title_packed.take();')]:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
  f=Path('src/store.rs');text=f.read_text();a=text.index('    pub fn row_maps_sync_updated(');b=text.index('\n    pub fn index_insert_at(',a);body=text[a:b];assert body.count(needle)==1;body=body.replace(needle,'/* intentional missing maintenance */');text=text[:a]+body+text[b:];f.write_text(text+'\n'+(p/'oracle.rs').read_text())
  with(p/f'mutant-{mutant}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','metadata_equality_oracle'],stdout=log,stderr=subprocess.STDOUT)
  log=(p/f'mutant-{mutant}.log').read_text();status.append(dict(mutant=mutant,exit_code=r.returncode));(p/'sensitivity-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==101 and 'assertion' in log and 'could not compile'not in log
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
