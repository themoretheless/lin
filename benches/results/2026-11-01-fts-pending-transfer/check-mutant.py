from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/fts.rs');original=source.read_bytes();text=original.decode();marker='        to.insert(key, list);';assert text.count(marker)==1
mutant=text.replace(marker,'        drop((key, list)); // deliberately lose publication')
(p/'missing-transfer-publication-mutant.rs').write_text(mutant)
r=None
try:
 source.write_text(mutant)
 with(p/'missing-transfer-publication-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','singleton_pending_transfer_reuses_storage_and_merges_sorted','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'missing-transfer-publication-runtime.log').read_text();assert r.returncode==101 and 'test result: FAILED.'in text and 'singleton_pending_transfer_reuses_storage_and_merges_sorted ... FAILED'in text
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(original_sha256=hashlib.sha256(original).hexdigest(),mutant_sha256=hashlib.sha256(mutant.encode()).hexdigest(),exit_code=r.returncode if r else None,source_restored=source.read_bytes()==original,runtime_assertion_caught=('test result: FAILED.'in text)),indent=2)+'\n')
print('Compiled missing-publication mutant rejected at runtime; source restored')
