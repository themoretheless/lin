#!/usr/bin/env python3
"""Audit retained execution order without changing historical acceptance results."""
import collections
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
ARCHIVES = [
    '2026-11-01-corrected-cow-full-engine-linux',
    '2026-11-01-linux-full-fsync-linux',
    '2026-11-01-fts-pending-transfer-linux',
]

def audit(name):
    path = f'benches/results/{name}/raw.tar.gz'
    proc = subprocess.Popen(['git', 'show', f'HEAD:{path}'], cwd=ROOT, stdout=subprocess.PIPE)
    found = []
    with tarfile.open(fileobj=proc.stdout, mode='r|gz') as archive:
        for member in archive:
            if member.isfile() and 'smoke' not in member.name and Path(member.name).name in ('process-status.json', 'benchmark-process-status.json'):
                data = archive.extractfile(member).read()
                found.append((member.name, data))
    assert proc.wait() == 0 and len(found) == 1, (name, [entry[0] for entry in found])
    member, data = found[0]
    records = json.loads(data)
    grouped = collections.defaultdict(lambda: collections.defaultdict(list))
    for record in records:
        case = record.get('id', record.get('case', record.get('workload')))
        role = record.get('mode', record.get('role'))
        assert case is not None and role in ('baseline', 'control', 'candidate')
        grouped[case][record['round']].append(role)
    details = {}
    for case, rounds in grouped.items():
        assert sorted(rounds) == list(range(1, 13))
        assert all(len(row) == 3 and len(set(row)) == 3 for row in rounds.values())
        counts = []
        for start in (1, 7):
            positions = {role: [sum(rounds[r][pos] == role for r in range(start, start+6))
                                for pos in range(3)] for role in ('baseline', 'control', 'candidate')}
            counts.append(positions)
        details[case] = dict(unique_orders=sorted(set(tuple(v) for v in rounds.values())),
                             series_position_counts=counts,
                             position_balanced=all(v == [2, 2, 2] for block in counts for v in block.values()))
    return dict(archive=path, member=member, process_status_sha256=hashlib.sha256(data).hexdigest(),
                processes=len(records), cases=len(details),
                fixed_order_cases=sum(len(v['unique_orders']) == 1 for v in details.values()),
                position_balanced_cases=sum(v['position_balanced'] for v in details.values()), details=details)

if __name__ == '__main__':
    results = [audit(name) for name in ARCHIVES]
    target = ROOT/'benches/results/2026-11-02-historical-role-order-audit'
    target.mkdir(exist_ok=True)
    (target/'audit.json').write_text(json.dumps(results, indent=2)+'\n')
    lines = ['# Historical execution-order audit', '',
             'Read actual process-status.json records from committed raw archives. Historical failed gates remain failed; numerical observations are retained. Position balance alone does not prove absence of other confounding.', '',
             '| Archive | Processes | Cases | Fixed order across 12 rounds | Balanced in both six-round series |',
             '| --- | ---: | ---: | ---: | ---: |']
    for result in results:
        lines.append(f"| {Path(result['archive']).parent.name} | {result['processes']} | {result['cases']} | {result['fixed_order_cases']} | {result['position_balanced_cases']} |")
    lines += ['', 'These source comparisons cannot establish a counterbalanced performance qualification. New qualification requires a frozen corrected scheduler; old outcomes must not be retrospectively accepted.']
    (target/'report.md').write_text('\n'.join(lines)+'\n')
    print('\n'.join(lines))
