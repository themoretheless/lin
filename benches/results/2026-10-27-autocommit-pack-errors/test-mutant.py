from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;s=p.parent/'2026-10-27-autocommit-pack-static';names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};status=[]
assert all(b==(s/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for mode in ['candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes((s/f'{"before"if mode=="baseline"else"candidate"}-{n}.rs').read_bytes())
  f=Path('src/exec.rs');text=f.read_text();needle='Err(e) => {\n                        self.rollback(undo, cat_backup);\n                        return Err(e);\n                    }\n                };\n            }\n            if recover_output_rows';assert text.count(needle)==1;f.write_text(text.replace(needle,needle.replace('self.rollback(undo, cat_backup);','drop(undo);')))
  with Path('src/exec.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
  with(p/f'mutant-{mode}-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','autocommit_boundary_oracle'],stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(variant=mode,exit_code=r.returncode));(p/'mutant-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode!=0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
