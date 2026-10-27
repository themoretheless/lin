from pathlib import Path
import json,hashlib,re,subprocess
p=Path(__file__).resolve().parent
h=lambda b:hashlib.sha256(b).hexdigest()
b=json.loads((p/'baseline-status.json').read_text());c=json.loads((p/'candidate-status.json').read_text())
assert b['exit_code']==101 and b['source_restored']
for f,digest in b['source_sha256'].items():assert h(subprocess.check_output(['git','show',b['source_commit']+':'+f]))==digest,f
base=(p/'baseline.log').read_text();assert '"reopen_ok":false' in base and 'catalog_hash mismatch' in base and 'panicked at' in base
assert c['exit_code']==0 and not c['performance_qualified']
for f,digest in c['source_sha256'].items():assert h(Path(f).read_bytes())==digest,f
assert h(Path('tests/schema_wal_rejection.rs').read_bytes())==c['regression_sha256']
log=(p/'full-tests.log').read_text();rows=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)
assert len(rows)==c['result_lines'] and all(r[0]=='ok' and r[2]=='0' for r in rows)
assert sum(int(r[1]) for r in rows)==c['reported_passed_including_child_processes']
assert sum(int(r[3]) for r in rows)==c['reported_ignored']
assert 'test rejected_schema_wal_preserves_catalog_hash_after_close ... ok' in log
print('PASS: actual baseline recovery failure, immutable baseline hashes, validated corrected core, full release log and final regression')
