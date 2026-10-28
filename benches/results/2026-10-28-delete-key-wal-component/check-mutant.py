from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');before=source.read_bytes();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};start=before.index(b'fn append_record_v1(');pos=before.index(b'    if buf.len() > MAX_RECORD as usize {',start);mutant=before[:pos]+before[pos:].replace(b'    if buf.len() > MAX_RECORD as usize {',b'    if false && buf.len() > MAX_RECORD as usize {',1);(p/'mutant-persist.rs').write_bytes(mutant);status=dict(original_source_sha256=hashes,mutant_sha256=hashlib.sha256(mutant).hexdigest(),runtime_mutant_rejected=False)
try:
 source.write_bytes(mutant)
 with(p/'mutant-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','delete_key_wal','canonical_oversize_delete_rejected_before_projection_or_file_write','--','--exact'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant-runtime.log').read_text();status.update(exit=r.returncode,runtime_mutant_rejected=r.returncode!=0 and 'test result: FAILED. 0 passed; 1 failed;'in text and 'panicked'in text)
finally:
 source.write_bytes(before);status['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'mutant-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['runtime_mutant_rejected']and status['all_sources_restored']
print('canonical quota bypass rejected at runtime; every source restored')
