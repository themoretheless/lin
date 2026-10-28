#!/usr/bin/env python3
"""Rehash deliberate evidence corruptions to exercise semantic audit checks."""
from pathlib import Path
import hashlib
import importlib.util
import json
import shutil
import tempfile

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('audit', root / 'scripts/audit-point-values-contract.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
original = audit.RESULT

def corrupt(path, operation):
    path.write_text(operation(path.read_text()))

mutations = [
    ('failed-positive', 'smoke/run.json', lambda s: json.dumps(dict(json.loads(s), status='failed'))),
    ('wrong-case', 'smoke/run.json', lambda s: s.replace('point_values/hit/lin', 'point_values/hit/other')),
    ('missing-sample', 'smoke/run.json', lambda s: json.dumps(dict(json.loads(s), observations=json.loads(s)['observations'][:-1]))),
    ('oracle-no-values', 'oracle.log', lambda s: s.replace('"full_value_checks":6', '"full_value_checks":0')),
    ('mutant-not-built', 'lin-mutant-build.log', lambda s: s.replace('Finished `bench` profile', 'compile failed')),
    ('mutant-not-rejected', 'lin-mutant-runtime.log', lambda s: s.replace('id/title bytes or multiplicity mismatch', 'unrelated failure')),
    ('wrong-mutant-source', 'lin-mutant.rs', lambda s: s.replace('row.get("id")', 'row.get("other")', 1)),
]
for label, name, operation in mutations:
    with tempfile.TemporaryDirectory(prefix='lin-point-audit-') as d:
        audit.RESULT = Path(d) / 'evidence'
        shutil.copytree(original, audit.RESULT)
        path = audit.RESULT / name
        corrupt(path, operation)
        manifest = json.loads((audit.RESULT / 'sha256.json').read_text())
        manifest[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        (audit.RESULT / 'sha256.json').write_text(json.dumps(manifest))
        try:
            audit.main()
        except AssertionError:
            print('PASS:', label)
        else:
            raise AssertionError('corruption accepted: ' + label)
audit.RESULT = original
audit.main()
