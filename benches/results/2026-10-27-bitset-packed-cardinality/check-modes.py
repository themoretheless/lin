from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes();rows=[]
oracle=(p/'oracle.rs').read_text().replace('fn exact_cardinality_stateful_model()', 'fn packed_validation_model()')+'\n'+(p/'capacity-oracle.rs').read_text().replace('fn tail_capacity_differential()', 'fn packed_validation_capacity()')
variants=['actual','limit0','limit1','limit2','limit16','forced-fallback','mutant-missing-capacity-decode']
try:
 for role in variants:
  s=(p/'candidate-index.rs').read_text()
  if role.startswith('limit'):
   needle='const PACKED_MAX_WORDS: usize = (u32::MAX as usize) / 64;';assert s.count(needle)==1;s=s.replace(needle,f'const PACKED_MAX_WORDS: usize = {role[5:]};')
  if role=='forced-fallback':s=s.replace('usize::BITS == 64 && self.bits.len()', 'false && self.bits.len()')
  if role.startswith('mutant'):
   needle='if self.packed() { self.capacity & u32::MAX as usize } else { self.capacity }';assert s.count(needle)==1;s=s.replace(needle,'self.capacity')
  f.write_text(s+'\n'+oracle)
  with(p/f'modes-{role}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','packed_validation'],stdout=log,stderr=subprocess.STDOUT)
  text=(p/f'modes-{role}.log').read_text()
  if role.startswith('mutant'):assert r.returncode!=0 and 'packed_validation_capacity ... FAILED'in text
  else:assert r.returncode==0 and '2 passed; 0 failed'in text
  rows.append(dict(role=role,exit_code=r.returncode));(p/'mode-status.json').write_text(json.dumps(dict(runs=rows),indent=2)+'\n');print(role,'verified',flush=True)
finally:
 f.write_bytes(before);(p/'mode-status.json').write_text(json.dumps(dict(runs=rows,source_restored=f.read_bytes()==before),indent=2)+'\n')
