from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;f=Path('src/store.rs');before=f.read_bytes();needle='self.as_map() == other.as_map()';s=before.decode();assert s.count(needle)==1
try:
 f.write_text(s.replace(needle,'!matches!((&self.0, &other.0), (RowStorage::Inline(_), RowStorage::Shared(_)) | (RowStorage::Shared(_), RowStorage::Inline(_))) && self.as_map() == other.as_map()'))
 with(p/'mutant.log').open('w') as log:r=subprocess.run(['cargo','test','--release','--lib','lazy_row_oracle::representation_preserves_eq_hash_wire_debug_and_snapshot_isolation','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant.log').read_text();assert r.returncode==101 and 'test result: FAILED' in text and 'panicked at' in text and 'error[E' not in text
 (p/'mutant-status.json').write_text(json.dumps(dict(exit_code=r.returncode,expected_failure=True,runtime_assertion_caught=True,mutant='cross-representation Eq falsely reports unequal'),indent=2)+'\n')
finally:f.write_bytes(before)
