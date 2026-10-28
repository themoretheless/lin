from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/embed.rs');before=f.read_bytes();needle=b'            for &i in &self.touched {\n                slots[i as usize].write(self.v[i as usize]);\n            }';assert before.count(needle)==1;mutant=before.replace(needle,b'            for &i in self.touched.iter().skip(1) {\n                slots[i as usize].write(self.v[i as usize]);\n            }');(p/'mutant-embed.rs').write_bytes(mutant);hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(original_source_sha256=hashes,mutant_sha256=hashlib.sha256(mutant).hexdigest(),runtime_mutant_rejected=False)
try:
 f.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','sparse_arc_output_preserves_dense_bits_thresholds_and_held_results','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();m.update(exit=r.returncode,runtime_mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'panicked'in text)
finally:
 f.write_bytes(before);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['runtime_mutant_rejected']and m['all_sources_restored']
print('missing output coordinate rejected at runtime; every source restored')
