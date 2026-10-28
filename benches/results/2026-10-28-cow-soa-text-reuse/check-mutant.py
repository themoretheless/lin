from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;src=Path('src/store.rs');original=src.read_bytes();source_hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
needle=b'            if !unchanged {\n                *slot = value.and_then(Cell::text_shared).unwrap_or_default();\n            }';assert original.count(needle)==1
mutant=original.replace(needle,b'            if false && !unchanged {\n                *slot = value.and_then(Cell::text_shared).unwrap_or_default();\n            }');(p/'mutant-store.rs').write_bytes(mutant)
status=dict(original_source_sha256=source_hashes,mutant_store_sha256=hashlib.sha256(mutant).hexdigest(),runtime_test='docs_reuse_and_changed_text_invalidate_packed_title',mutant_rejected=False)
try:
 src.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','soa_text_reuse','docs_reuse_and_changed_text_invalidate_packed_title','--','--exact'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();status.update(exit=r.returncode,mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'assertion `left == right` failed'in text)
finally:
 src.write_bytes(original);status['all_production_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in source_hashes.items());(p/'mutant-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['all_production_sources_restored']and status['mutant_rejected'],status
print('incorrect SoA update elision rejected at runtime; every source restored')
