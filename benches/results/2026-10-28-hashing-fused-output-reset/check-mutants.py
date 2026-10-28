from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/embed.rs');before=f.read_bytes();hashes={str(x):hashlib.sha256(x.read_bytes()).hexdigest()for x in Path('src').rglob('*.rs')}
needle=b'            for &i in &self.touched {\n                slots[i as usize].write(self.v[i as usize]);\n                self.v[i as usize] = 0.0;\n                self.seen[i as usize] = false;\n            }';assert before.count(needle)==1
variants={
 'missing_coordinate':needle.replace(b'slots[i as usize].write(self.v[i as usize]);',b'if i != self.touched[0] { slots[i as usize].write(self.v[i as usize]); }'),
 'stale_value':needle.replace(b'self.v[i as usize] = 0.0;',b'// intentionally retained scratch value'),
 'stale_seen':needle.replace(b'self.seen[i as usize] = false;',b'// intentionally retained seen bit'),
}
records=[]
try:
 for name,replacement in variants.items():
  mutant=before.replace(needle,replacement);(p/('mutant-'+name+'.rs')).write_bytes(mutant);f.write_bytes(mutant)
  with(p/('mutant-'+name+'.log')).open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','sparse_arc_output_preserves_dense_bits_thresholds_and_held_results','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  text=(p/('mutant-'+name+'.log')).read_text();rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'panicked'in text
  records.append(dict(name=name,exit_code=r.returncode,runtime_mutant_rejected=rejected,mutant_sha256=hashlib.sha256(mutant).hexdigest()));assert rejected,name
  f.write_bytes(before);print(name,'runtime rejected',flush=True)
finally:
 f.write_bytes(before);restored=all(hashlib.sha256(Path(x).read_bytes()).hexdigest()==h for x,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(dict(original_source_sha256=hashes,mutants=records,all_sources_restored=restored),indent=2)+'\n')
assert len(records)==3 and all(x['runtime_mutant_rejected']for x in records)and restored
