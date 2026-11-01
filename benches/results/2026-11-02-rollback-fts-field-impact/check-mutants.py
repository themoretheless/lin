"""Compile both broken FTS-impact decisions; require named runtime failures."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

p = Path(__file__).resolve().parent
root = p.parents[2]
source = root/'src/exec.rs'
before = source.read_bytes()
hashes = {str(f.relative_to(root)): hashlib.sha256(f.read_bytes()).hexdigest() for f in (root/'src').glob('*.rs')}
needle = b'self.0.iter().any(|(key, _)| key == "snippet" || fields.contains(key))'
assert before.count(needle) == 1
env = dict(os.environ)
env.setdefault('CARGO_TARGET_DIR', str(root/'target/rollback-fts-field-impact'))
env.setdefault('CARGO_BUILD_JOBS', '2')
results = []
try:
    for name, value, test in [
        ('missing_restore', b'false', 'rollback_fts_journal_impact_matches_rebuild_after_folding'),
        ('unnecessary_restore', b'true', 'rollback_fts_unrelated_patch_keeps_base_postings'),
    ]:
        mutant = before.replace(needle, value)
        (p/(name+'-mutant.rs')).write_bytes(mutant)
        source.write_bytes(mutant)
        qualified = 'rollback_fts_field_impact_tests::'+test
        command = ['cargo', 'test', '--release', '--lib', qualified, '--', '--exact', '--nocapture']
        log = p/(name+'-runtime.log')
        with log.open('w') as output:
            run = subprocess.run(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text()
        caught = run.returncode == 101 and qualified+' ... FAILED' in text and 'test result: FAILED. 0 passed; 1 failed;' in text
        results.append(dict(name=name, command=command, exit_code=run.returncode, named_runtime_failure=caught,
                            mutant_sha256=hashlib.sha256(mutant).hexdigest(), log_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
        assert caught, (name, run.returncode)
finally:
    source.write_bytes(before)
    restored = {str(f.relative_to(root)): hashlib.sha256(f.read_bytes()).hexdigest() for f in (root/'src').glob('*.rs')} == hashes
    (p/'mutant-status.json').write_text(json.dumps(dict(source_restored=restored, source_sha256=hashes,
        checks=results, performance_qualification=False, full_objective_achieved=False), indent=2)+'\n')
    assert restored
print('PASS both compiled mutants rejected by named runtime assertions; all source bytes restored')
