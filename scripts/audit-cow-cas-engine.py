#!/usr/bin/env python3
"""Audit a completed frozen COW experiment without resetting current project sources."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
INPUT = 'benches/results/2026-10-28-cow-cas-full-engine'
HEAD = '36666994a36ac532122a0de2cac3ad12d869d8ec'

def digest(data):
    return hashlib.sha256(data).hexdigest()

def git_bytes(name):
    relative = Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts, name
    return subprocess.check_output(['git', 'show', HEAD + ':' + name], cwd=ROOT)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('result_directory', type=Path, help='parent containing run-metadata.json and downloaded cow-vnext-full-engine artifact')
    args = parser.parse_args()
    parent = args.result_directory.resolve()
    artifact = parent / 'cow-cas-full-engine'
    metadata = json.loads((parent / 'run-metadata.json').read_text())
    assert metadata['status'] == 'completed' and metadata['headSha'] == HEAD
    assert metadata['conclusion'] in {'success', 'failure'}, 'Cancelled/incomplete runs cannot qualify a source'
    manifest_bytes = (artifact / 'input-provenance.json').read_bytes()
    assert manifest_bytes == git_bytes(INPUT + '/input-provenance.json'), 'CI input manifest drift'
    manifest = json.loads(manifest_bytes)
    for name, expected in manifest['files_sha256'].items():
        assert digest((artifact / name).read_bytes()) == expected, name
    provenance = json.loads((artifact / 'provenance.json').read_text())
    assert provenance['source_restored'] and provenance['source_sha256'] == manifest['source_sha256']
    assert provenance['compiled_compare_sha256'] == manifest['files_sha256']['compare.rs']
    assert manifest['baseline_reference'] == '6812cc5ca9fc35a4adc81e0948df45dfe3629742'
    assert manifest['public_row_newtype_break'] and manifest['api_migration_authorized'] and manifest['single_statement_cas_snapshot_elision']
    for name in ['exec', 'store', 'check', 'index']:
        assert provenance['candidate_sources_sha256']['src/' + name + '.rs'] == manifest['files_sha256']['candidate-' + name + '.rs']
        assert digest(git_bytes('src/' + name + '.rs')) == manifest['files_sha256']['candidate-' + name + '.rs']
        assert provenance['baseline_sources_sha256']['src/' + name + '.rs'] == manifest['files_sha256']['baseline-' + name + '.rs']
        baseline = subprocess.check_output(['git', 'show', manifest['baseline_reference'] + ':src/' + name + '.rs'], cwd=ROOT)
        assert digest(baseline) == manifest['files_sha256']['baseline-' + name + '.rs']
    records = json.loads((artifact / 'process-status.json').read_text())
    assert len(records) == 2412, 'Incomplete timing matrix; preserve artifact without source qualification'
    qualification = json.loads((artifact / 'qualification.json').read_text())
    assert qualification['measurement_complete'] and not qualification['full_objective_achieved']
    assert qualification['scalar_component_gate_passes'] is True
    assert qualification['target_cases'] == 5 and qualification['control_cases'] == 62
    assert qualification['processes'] == 2412 and qualification['observations'] == 19296
    logs = []
    with tempfile.TemporaryDirectory(prefix='lin-frozen-wal-audit-') as temporary:
        snapshot = Path(temporary)
        paths = dict(manifest['source_sha256'], **manifest['build_inputs_sha256'])
        paths['benches/compare.rs'] = provenance['original_compare_sha256']
        for name, expected in paths.items():
            data = git_bytes(name)
            assert digest(data) == expected, name
            destination = snapshot / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        for verifier in ['verify-functional.py', 'verify.py']:
            result = subprocess.run(['python3', str(artifact / verifier)], cwd=snapshot, text=True, capture_output=True)
            logs.append(f'{verifier}: exit={result.returncode}\n{result.stdout}{result.stderr}')
            assert result.returncode == 0, logs[-1]
    summary = json.loads((artifact / 'summary.json').read_text())
    failures = [dict(case=case, series=series, **record) for case, series_rows in summary.items()
                for series, record in series_rows.items() if not record['passes']]
    peers = json.loads((artifact / 'peer-comparisons.json').read_text())
    peer_counts = {}
    for row in peers:
        entry = peer_counts.setdefault(row['peer'], dict(wins=0, comparisons=0))
        entry['wins'] += row['lin_wins']
        entry['comparisons'] += row['pairs']
    result = dict(audit_complete=True, run_head=HEAD, source_candidate_qualified=qualification['source_candidate_qualified'],
                  full_objective_achieved=False, processes=len(records), observations=qualification['observations'],
                  failed_target_windows=sum(row['target'] for row in failures),
                  failed_control_windows=sum(not row['target'] for row in failures),
                  failed_windows=failures, candidate_peer_counts=peer_counts,
                  scope='Frozen Linux combined COW and single-statement CAS experiment; five targets and62controls; public API transition authorized; scalar component gate passed; eight-peer objective unproven')
    raw_report = (artifact / 'report.md').read_text()
    assert raw_report.startswith('# COW and single-statement CAS: full-engine paired gate\n'), 'Unexpected frozen report format'
    (parent / 'audited-report.md').write_text(raw_report)
    (parent / 'independent-audit.log').write_text('\n'.join(logs))
    (parent / 'independent-audit.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'failed_windows'}, indent=2))

if __name__ == '__main__':
    main()
