#!/usr/bin/env python3
"""Audit a completed frozen WAL experiment without resetting current project sources."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
INPUT = 'benches/results/2026-10-27-wal-zero-full-engine'
HEAD = 'c464574c5ef43afeb8afd858235078ca3329390a'

def digest(data):
    return hashlib.sha256(data).hexdigest()

def git_bytes(name):
    relative = Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts, name
    return subprocess.check_output(['git', 'show', HEAD + ':' + name], cwd=ROOT)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('result_directory', type=Path, help='parent containing run-metadata.json and downloaded wal-zero-full-engine artifact')
    args = parser.parse_args()
    parent = args.result_directory.resolve()
    artifact = parent / 'wal-zero-full-engine'
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
    assert provenance['candidate_persist_sha256'] == manifest['files_sha256']['candidate-persist.rs']
    records = json.loads((artifact / 'process-status.json').read_text())
    assert len(records) == 2268, 'Incomplete timing matrix; preserve artifact without source qualification'
    qualification = json.loads((artifact / 'qualification.json').read_text())
    assert qualification['measurement_complete'] and not qualification['full_objective_achieved']
    assert qualification['cpu_component_gate_passes'] is False
    assert qualification['target_cases'] == 6 and qualification['control_cases'] == 57
    assert qualification['processes'] == 2268 and qualification['observations'] == 18144
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
                  scope='Frozen Linux bounded positive-zero full-engine experiment only; does not establish the full eight-peer objective')
    (parent / 'independent-audit.log').write_text('\n'.join(logs))
    (parent / 'independent-audit.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'failed_windows'}, indent=2))

if __name__ == '__main__':
    main()
