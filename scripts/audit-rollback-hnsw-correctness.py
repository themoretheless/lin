#!/usr/bin/env python3
"""Independently verify frozen rollback correction sources and Linux raw assertions."""
from pathlib import Path
import argparse
import hashlib
import json
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REF = '21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5'
PREFIX = 'benches/results/2026-10-28-rollback-hnsw-correctness/'

def blob(name, ref=REF):
    return subprocess.check_output(['git', 'show', f'{ref}:{name}'], cwd=ROOT)

def audit(raw):
    raw = Path(raw)
    inputs = json.loads(blob(PREFIX + 'frozen-inputs.json'))
    for name, digest in inputs.items():
        assert hashlib.sha256(blob(name)).hexdigest() == digest, name
    status = json.loads((raw / 'status.json').read_text())
    assert status['source_commit'] == REF and status['sources_restored']
    assert status['release_exit'] == 0 and status['release_passed'] == 454 and status['release_ignored'] == 2
    assert 'rustc 1.96.0 ' in status['compiler'] and 'host: x86_64-unknown-linux-gnu' in status['compiler']
    assert not status['performance_qualified']
    log = (raw / 'full-release.log').read_text()
    counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
    assert counts and sum(int(c[0]) for c in counts) == 454 and sum(int(c[2]) for c in counts) == 2
    assert not any(int(c[1]) for c in counts) and 'test result: FAILED.' not in log
    store = blob('src/store.rs').decode()
    start = store.index('        // Appended vector nodes must not outlive')
    end = store.index('        self.next_id = mark.next_id;', start)
    needle = '            self.hnsw_insert_slab(collection, p, std::slice::from_ref(doomed));\n'
    assert store.count(needle) == 1
    expected = {
        'missing-delete-node': (store.replace(needle, ''), 'rollback_restores_hnsw_after_delete'),
        'stale-appended-node': (store[:start] + store[end:], 'append_rollback_removes_hnsw_nodes_at_truncated_slots'),
        'unordered-append-journal': (blob('src/exec.rs', '292b93fac').decode(), 'rollback_interleaved_append_delete_preserves_rows_and_keys'),
    }
    checks = status['runtime_negatives']
    assert len(checks) == 3 and {c['label'] for c in checks} == set(expected)
    for label, (source, test) in expected.items():
        assert (raw / (label + '.rs')).read_text() == source, label
        check = next(c for c in checks if c['label'] == label)
        assert check['exit'] == 101 and check['runtime_rejected']
        text = (raw / (label + '.log')).read_text()
        assert 'running 1 test' in text and 'test result: FAILED.' in text
        assert re.search(r'test [^\n]*' + re.escape(test) + r' \.\.\. FAILED', text), label
        assert 'Finished `release` profile' in text, label
    return dict(audit_complete=True, source_commit=REF, release_passed=454, release_ignored=2, runtime_negatives_rejected=3, performance_qualified=False, full_objective_achieved=False)

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('raw', type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.raw), indent=2))
