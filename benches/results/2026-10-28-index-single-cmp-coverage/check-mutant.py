from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes();needle=b'        return index_covers_conjunct(pred, use_);';assert before.count(needle)==1
hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};mutant=before.replace(needle,b'        return true;');(p/'mutant-index.rs').write_bytes(mutant);m=dict(original_source_sha256=hashes,mutant_sha256=hashlib.sha256(mutant).hexdigest(),runtime_mutant_rejected=False)
try:
 f.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','single_cmp_coverage_tests::atomic_coverage_matches_dnf_with_ranges_partial_indexes_and_precision_edges','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();m.update(exit=r.returncode,runtime_mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'assertion `left == right` failed'in text)
finally:
 f.write_bytes(before);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['runtime_mutant_rejected']and m['all_sources_restored']
print('unsafe unconditional index coverage rejected by runtime reference test; all sources restored')
