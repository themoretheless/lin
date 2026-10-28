from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/embed.rs');before=f.read_bytes();needle=b'                    out.extend(worker.join().expect("hashing batch worker panicked"));';assert before.count(needle)==1;mutant=before.replace(needle,b'                    let mut wrong_order = worker.join().expect("hashing batch worker panicked");\n                    wrong_order.reverse();\n                    out.extend(wrong_order);');(p/'mutant-embed.rs').write_bytes(mutant);hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(original_source_sha256=hashes,mutant_sha256=hashlib.sha256(mutant).hexdigest(),runtime_mutant_rejected=False)
try:
 f.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','parallel_hashing_batch_tests::parallel_vectors_match_serial_and_single_bits_in_input_order','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();m.update(exit=r.returncode,runtime_mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'panicked'in text)
finally:
 f.write_bytes(before);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['runtime_mutant_rejected']and m['all_sources_restored']
print('reversed worker output rejected at runtime; every source restored')
