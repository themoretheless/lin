from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/index.rs');before=source.read_bytes();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};start=before.index(b'fn index_covers_single_cmp(');guard=before.index(b'            if matches!(value, Value::Int',start);end=before.index(b'            use_.eqs.iter()',guard);mutant=before[:guard]+b'            // Deliberately omit numeric precision guard.\n'+before[end:];(p/'mutant-index.rs').write_bytes(mutant);m=dict(original_source_sha256=hashes,mutant_sha256=hashlib.sha256(mutant).hexdigest(),runtime_mutant_rejected=False)
try:
 source.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','allocation_free_cmp_coverage_tests::atomic_coverage_matches_dnf_with_ranges_partial_indexes_and_precision_edges','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();m.update(exit=r.returncode,runtime_mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'assertion `left == right` failed'in text)
finally:
 source.write_bytes(before);m['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['runtime_mutant_rejected']and m['all_sources_restored']
print('numeric precision bypass rejected at runtime; every source restored')
