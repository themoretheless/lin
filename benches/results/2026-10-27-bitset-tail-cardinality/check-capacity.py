from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes();rows=[]
try:
 for role in ['candidate','mutant-metadata-growth']:
  text=(p/'candidate-index.rs').read_text()
  if role.startswith('mutant'):
   needle='        self.bits[old_words] = 0;';assert text.count(needle)==1;text=text.replace(needle,'        // deliberate missing metadata clear')
  f.write_text(text+'\n'+(p/'capacity-oracle.rs').read_text())
  with(p/f'{role}-capacity.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','tail_capacity_differential'],stdout=log,stderr=subprocess.STDOUT)
  if role=='candidate':assert r.returncode==0
  else:assert r.returncode!=0 and 'tail_capacity_differential ... FAILED'in(p/f'{role}-capacity.log').read_text()
  rows.append(dict(role=role,exit_code=r.returncode))
finally:
 f.write_bytes(before);(p/'capacity-status.json').write_text(json.dumps(dict(runs=rows,source_restored=f.read_bytes()==before),indent=2)+'\n')
