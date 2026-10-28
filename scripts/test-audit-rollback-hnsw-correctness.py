#!/usr/bin/env python3
"""Corrupt downloaded Linux evidence to verify independent-audit sensitivity."""
from pathlib import Path
import importlib.util
import json
import re
import shutil
import sys
import tempfile

spec = importlib.util.spec_from_file_location('audit', Path(__file__).with_name('audit-rollback-hnsw-correctness.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
raw = Path(sys.argv[1])
module.audit(raw)

def status_edit(text, **changes):
    return json.dumps(dict(json.loads(text), **changes))

mutations = [
    ('wrong-source', 'status.json', lambda s: status_edit(s, source_commit='0'*40)),
    ('wrong-count', 'status.json', lambda s: status_edit(s, release_passed=453)),
    ('not-restored', 'status.json', lambda s: status_edit(s, sources_restored=False)),
    ('wrong-platform', 'status.json', lambda s: s.replace('x86_64-unknown-linux-gnu', 'aarch64-apple-darwin')),
    ('changed-mutant', 'missing-delete-node.rs', lambda s: s+'\n'),
    ('no-runtime-failure', 'stale-appended-node.log', lambda s: s.replace('test result: FAILED.', 'compile error')),
    ('unrelated-failing-test', 'unordered-append-journal.log', lambda s: s.replace('rollback_interleaved_append_delete_preserves_rows_and_keys', 'unrelated_test')),
    ('missing-full-result', 'full-release.log', lambda s: re.sub(r'test result: ok\. \d+ passed; \d+ failed; \d+ ignored[^\n]*', '', s, count=1)),
]
for label, name, change in mutations:
    with tempfile.TemporaryDirectory(prefix='lin-rollback-audit-') as td:
        dest = Path(td) / 'raw'
        shutil.copytree(raw, dest)
        path = dest / name
        before = path.read_text()
        changed = change(before)
        assert changed != before, label
        path.write_text(changed)
        try:
            module.audit(dest)
        except AssertionError:
            print('PASS:', label)
        else:
            raise AssertionError('Corrupt evidence accepted: '+label)
print('PASS: valid raw package and eight semantic corruptions')
