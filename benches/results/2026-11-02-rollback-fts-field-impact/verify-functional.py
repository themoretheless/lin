from pathlib import Path
import hashlib
import json
import re
import subprocess

p = Path(__file__).resolve().parent
root = p.parents[2]
digest = lambda f: hashlib.sha256(f.read_bytes()).hexdigest()
source = json.loads((p/'source-before-tests.json').read_text())
assert all(digest(root/name) == h for name, h in source.items())
for name in ['exec.rs', 'store.rs', 'lib.rs', 'rollback_fts_field_impact_tests.rs']:
    assert (root/'src'/name).read_bytes() == (p/('candidate-'+name)).read_bytes()
changed = {'src/exec.rs', 'src/store.rs', 'src/lib.rs', 'src/rollback_fts_field_impact_tests.rs'}
for name, h in source.items():
    if name not in changed:
        assert hashlib.sha256(subprocess.check_output(['git', 'show', '625bc7180:'+name], cwd=root)).hexdigest() == h
status = json.loads((p/'test-status.json').read_text())
assert status['source_sha256'] == source and status['source_restored'] and status['exit_code'] == 0
log = (p/'full-release.log').read_text()
counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
assert [sum(int(row[i]) for row in counts) for i in range(3)] == [460, 0, 2]
assert status['passed'] == 460 and status['failed'] == 0 and status['ignored'] == 2
names = ['rollback_fts_journal_impact_matches_rebuild_after_folding', 'rollback_fts_unrelated_patch_keeps_base_postings']
restored = (p/'restored-focused.log').read_text()
for name in names:
    marker = 'rollback_fts_field_impact_tests::'+name+' ... ok'
    assert marker in log and marker in restored
assert 'test result: ok. 2 passed; 0 failed; 0 ignored;' in restored
negative = json.loads((p/'mutant-status.json').read_text())
assert negative['source_restored'] and negative['source_sha256'] == source and len(negative['checks']) == 2
original = (root/'src/exec.rs').read_bytes()
needle = b'self.0.iter().any(|(key, _)| key == "snippet" || fields.contains(key))'
assert original.count(needle) == 1
for record, (name, value, test) in zip(negative['checks'], [
    ('missing_restore', b'false', names[0]), ('unnecessary_restore', b'true', names[1]),
]):
    qualified = 'rollback_fts_field_impact_tests::'+test
    mutant = p/(name+'-mutant.rs'); runtime_log = p/(name+'-runtime.log')
    assert mutant.read_bytes() == original.replace(needle, value)
    assert digest(mutant) == record['mutant_sha256'] and digest(runtime_log) == record['log_sha256']
    assert record['name'] == name and record['exit_code'] == 101 and record['named_runtime_failure']
    assert record['command'] == ['cargo', 'test', '--release', '--lib', qualified, '--', '--exact', '--nocapture']
    text = runtime_log.read_text()
    assert qualified+' ... FAILED' in text and 'test result: FAILED. 0 passed; 1 failed;' in text
result = dict(complete=True, passed=460, ignored=2, compiled_runtime_negatives=2,
              restored_positive_tests=2, source_restored=True, performance_qualification=False,
              full_objective_achieved=False)
(p/'functional-audit.json').write_text(json.dumps(result, indent=2)+'\n')
print('PASS source-bound full release suite, both compiled negatives and restored positive checks; no speed qualification')
