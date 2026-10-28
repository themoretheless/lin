"""Frozen Linux correctness run and three independently compiled runtime negatives."""
from pathlib import Path
import hashlib
import json
import re
import subprocess

root = Path(__file__).resolve().parents[3]
result = Path(__file__).resolve().parent / 'linux'
result.mkdir(exist_ok=True)
inputs = json.loads((result.parent / 'frozen-inputs.json').read_text())
for name, digest in inputs.items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name

def execute(label, args):
    with (result / (label + '.log')).open('w') as log:
        proc = subprocess.run(args, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    return proc.returncode, (result / (label + '.log')).read_text()

status = dict(compiler=subprocess.check_output(['rustc', '-Vv'], cwd=root, text=True), source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(), performance_qualified=False)
code, text = execute('full-release', ['cargo', 'test', '--release', '--color', 'never'])
status['release_exit'] = code
counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', text)
assert code == 0 and counts and not any(int(c[1]) for c in counts), 'full release suite failed'
status['release_passed'] = sum(int(c[0]) for c in counts)
status['release_ignored'] = sum(int(c[2]) for c in counts)
assert status['release_passed'] == 454 and status['release_ignored'] == 2
store = root / 'src/store.rs'
executor = root / 'src/exec.rs'
original_store = store.read_text()
original_executor = executor.read_text()
start = original_store.index('        // Appended vector nodes must not outlive')
end = original_store.index('        self.next_id = mark.next_id;', start)
old_executor = subprocess.check_output(['git', 'show', '292b93fac:src/exec.rs'], cwd=root, text=True)
mutants = [
    ('missing-delete-node', store, original_store.replace('            self.hnsw_insert_slab(collection, p, std::slice::from_ref(doomed));\n', ''), ['--lib', 'rollback_restores_hnsw_after_delete']),
    ('stale-appended-node', store, original_store[:start] + original_store[end:], ['--lib', 'append_rollback_removes_hnsw_nodes_at_truncated_slots']),
    ('unordered-append-journal', executor, old_executor, ['--test', 'transaction_tests', 'rollback_interleaved_append_delete_preserves_rows_and_keys']),
]
checks = []
try:
    for label, path, mutant, select in mutants:
        assert mutant != path.read_text()
        path.write_text(mutant)
        (result / (label + '.rs')).write_text(mutant)
        code, text = execute(label, ['cargo', 'test', '--release', '--color', 'never'] + select)
        assert code == 101 and 'test result: FAILED.' in text and 'running 1 test' in text and ' ... FAILED' in text, label + ': expected compiled runtime assertion failure'
        checks.append(dict(label=label, exit=code, runtime_rejected=True))
        store.write_text(original_store)
        executor.write_text(original_executor)
finally:
    store.write_text(original_store)
    executor.write_text(original_executor)
    status['runtime_negatives'] = checks
    status['sources_restored'] = all(hashlib.sha256((root / name).read_bytes()).hexdigest() == digest for name, digest in inputs.items())
    (result / 'status.json').write_text(json.dumps(status, indent=2) + '\n')
assert len(checks) == 3 and status['sources_restored']
print(json.dumps(status))
