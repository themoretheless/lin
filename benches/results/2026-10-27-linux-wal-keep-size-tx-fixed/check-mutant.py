from pathlib import Path
import json,subprocess,platform,hashlib
p=Path(__file__).resolve().parent;f=Path('src/persist.rs');before=f.read_bytes();candidate=(p/'candidate-persist.rs').read_text();linux=platform.system()=='Linux'
if linux:
 assert candidate.count('libc::FALLOC_FL_KEEP_SIZE')==1;candidate=candidate.replace('libc::FALLOC_FL_KEEP_SIZE','0');target='linux_keep_size_oracle::actual_keep_size_preserves_eof_cursor_prefix_and_reports_support';kind='KEEP_SIZE flag removed'
else:
 needle='Some((i64::try_from(logical).ok()?, i64::try_from(length).ok()?))';assert candidate.count(needle)==1;candidate=candidate.replace(needle,'Some((0, i64::try_from(length).ok()?))');target='linux_keep_size_model';kind='range incorrectly starts at zero'
try:
 f.write_text(candidate+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib',target,'--color','never','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'mutant.log').read_text();assert r.returncode!=0 and 'test result: FAILED'in text and 'error[E'not in text
 (p/'mutant-status.json').write_text(json.dumps(dict(exit_code=r.returncode,expected_failure=True,runtime_assertion_caught=True,mutant=kind,linux_backend_executed=linux),indent=2)+'\n')
finally:
 f.write_bytes(before)
