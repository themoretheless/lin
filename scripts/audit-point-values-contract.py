#!/usr/bin/env python3
"""Audit retained equivalent-value correctness evidence, never infer latency qualification."""
from pathlib import Path
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
RESULT = ROOT / 'benches/results/2026-10-28-point-values-contract'
REF = '63d8a7f53'

def read(name):
    return json.loads((RESULT / name).read_text())

def blob(name):
    return subprocess.check_output(['git', 'show', f'{REF}:{name}'], cwd=ROOT)

def main():
    manifest = read('sha256.json')
    actual = {str(p.relative_to(RESULT)) for p in RESULT.rglob('*') if p.is_file() and p.name != 'sha256.json'}
    assert set(manifest) == actual, 'raw file set changed'
    for name, digest in manifest.items():
        assert hashlib.sha256((RESULT / name).read_bytes()).hexdigest() == digest, name
    provenance = read('input.json')
    for name, digest in provenance['source_sha256'].items():
        assert hashlib.sha256(blob(name)).hexdigest() == digest, name
    source = blob('benches/point_values.rs').decode()
    assert source.count('.bench_checked(') == 3 and 'DropPolicy::InsideTiming' in source
    assert 'actual == expected' in source and 'Vec<Pair>' in source
    assert 'Finished `bench` profile' in (RESULT / 'build-positive.log').read_text()
    oracle = json.loads((RESULT / 'oracle.log').read_text())
    assert oracle == dict(full_value_checks=6, negative_oracle_checks=5, performance_qualified=False)
    run = read('smoke/run.json')
    cases = {f'point_values/{kind}/{engine}' for kind in ['hit', 'miss'] for engine in ['lin', 'sqlite', 'duckdb']}
    assert run['status'] == 'complete' and {c['id'] for c in run['cases']} == cases
    assert len(run['observations']) == 6
    assert {o['case'] for o in run['observations']} == cases
    assert all(o['availability']['state'] == 'available' and o['operations'] == 1 for o in run['observations'])
    assert all(c['contract']['validation'] == 'fresh operation before calibration and after measurement' for c in run['cases'])
    mutants = read('mutant-status.json')
    assert mutants['source_restored'] and not mutants['performance_qualified']
    assert len(mutants['checks']) == 3
    for engine, fn, old, new in [('lin', 'lin_rows', 'row.get("title")', 'row.get("id")'), ('sqlite', 'sqlite_rows', 'row.get::<_, String>(1)', 'row.get::<_, String>(0)'), ('duckdb', 'duck_rows', 'row.get::<_, String>(1)', 'row.get::<_, String>(0)')]:
        start = source.index('fn ' + fn + '(')
        end = source.index('\n}', start) + 2
        body = source[start:end]
        assert body.count(old) == 1
        expected = source[:start] + body.replace(old, new) + source[end:]
        assert (RESULT / f'{engine}-mutant.rs').read_text() == expected
        check = next(c for c in mutants['checks'] if c['engine'] == engine)
        assert check['build_exit'] == 0 and check['runtime_exit'] == 1 and check['wrong_title_caught']
        assert 'Finished `bench` profile' in (RESULT / f'{engine}-mutant-build.log').read_text()
        assert 'id/title bytes or multiplicity mismatch' in (RESULT / f'{engine}-mutant-runtime.log').read_text()
    print('PASS: frozen sources, raw hashes, six full-value cases and three runtime mutants; no performance qualification')

if __name__ == '__main__':
    main()
