from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent;f=Path('src/persist.rs');before=f.read_bytes();candidate=(p/'candidate-persist.rs').read_text();needle='value.to_bits() == 0';assert candidate.count(needle)==2;candidate=candidate.replace(needle,'value.abs().to_bits() == 0')
try:
 f.write_text(candidate+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','persist::wal_positive_zero_oracle::legacy_decoder_preserves_complete_records_and_ieee_bits','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant.log').read_text();assert r.returncode!=0 and 'test result: FAILED'in text and 'panicked at'in text and 'error[E'not in text
 (p/'mutant-status.json').write_text(json.dumps(dict(exit_code=r.returncode,expected_failure=True,runtime_assertion_caught=True,mutant='negative zero incorrectly treated as positive zero'),indent=2)+'\n')
finally:
 f.write_bytes(before)
