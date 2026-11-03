"""Independent frozen-gate reconstruction. No timing or executable invocation."""
from pathlib import Path
import argparse, hashlib, json, math, statistics as st, subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('raw', type=Path)
args = parser.parse_args()
p = Path(__file__).resolve().parent
s = json.loads((p/'input.json').read_text())
r = args.raw
statuses = json.loads((r/'process-status.json').read_text())
assert len(statuses) == 36
assert {(x['round'], x['role']) for x in statuses} == {
    (i, role) for i in range(1, 13) for role in ['baseline', 'control', 'candidate']}
assert [name for name in s['current_source_sha256'] if s['current_source_sha256'][name] != s['baseline_source_sha256'][name]] == ['src/exec.rs']
for role in ['baseline', 'candidate']:
    info = s['binaries'][role]
    assert hashlib.sha256(Path(info['path']).read_bytes()).hexdigest() == info['sha256']
    ref = s['baseline_source_commit'] if role == 'baseline' else s['candidate_runtime_commit']
    source = s['baseline_source_sha256'] if role == 'baseline' else s['current_source_sha256']
    for name, digest in source.items():
        assert hashlib.sha256(subprocess.check_output(['git', 'show', ref+':'+name])).hexdigest() == digest
cases = {f'compare/durable_insert_wal_{n}/{e}' for n in ['1k', '10k'] for e in ['lin', 'sqlite']}
values = {}
count = 0
for row in statuses:
    i, role = row['round'], row['role']
    info = s['binaries']['baseline' if role == 'control' else role]
    assert row['exit'] == 0 and row['binary_sha256'] == info['sha256']
    d = json.loads((r/f'{i:02}-{role}/run.json').read_text())
    assert d['status'] == 'complete' and len(d['observations']) == 96
    assert {c['id'] for c in d['cases']} == cases
    for c in d['cases']:
        assert c['contract']['validation'] == 'fresh operation before calibration and after measurement'
        obs = [o for o in d['observations'] if o['case'] == c['id']]
        assert len(obs) == 24
        for o in obs:
            assert o['metric'] == 'wall' and o['operations'] == 1 and o['availability']['state'] == 'available'
            assert math.isfinite(float(o['value'])) and float(o['value']) > 0
        values[i, role, c['id']] = st.median(float(o['value']) for o in obs)
        count += len(obs)
assert count == 3456
summary = json.loads((r/'summary.json').read_text())
assert len(summary) == 8
assert {(x['case'], x['series']) for x in summary} == {(case, series) for case in cases for series in [1, 2]}
for row in summary:
    start = 1 if row['series'] == 1 else 7
    case = row['case']
    gains = [100*(1-values[i, 'candidate', case]/values[i, 'baseline', case]) for i in range(start, start+6)]
    aa = [abs(100*(1-values[i, 'control', case]/values[i, 'baseline', case])) for i in range(start, start+6)]
    assert gains == row['gains_percent'] and aa == row['aa_absolute_percent']
    assert row['median_gain_percent'] == st.median(gains) and row['median_aa_percent'] == st.median(aa)
    target = case == 'compare/durable_insert_wal_1k/lin'
    assert row['target'] == target and row['positive_pairs'] == sum(x > 0 for x in gains)
    passed = (sum(x > 0 for x in gains) == 6 and st.median(gains) > st.median(aa)) if target else st.median(gains) >= -max(2, st.median(aa))
    assert passed == row['passes']
peer = json.loads((r/'peer-comparisons.json').read_text())
assert len(peer) == 2 and {x['rows'] for x in peer} == {'1k', '10k'}
for row in peer:
    n = row['rows']
    ratios = [values[i, 'candidate', f'compare/durable_insert_wal_{n}/sqlite'] /
              values[i, 'candidate', f'compare/durable_insert_wal_{n}/lin'] for i in range(1, 13)]
    assert row['sqlite_over_lin_process_ratios'] == ratios
    assert row['positive_processes'] == sum(x > 1 for x in ratios)
decision = json.loads((r/'decision.json').read_text())
assert decision == dict(complete=True, processes=36, observations=3456,
                       component_gate_passes=all(x['passes'] for x in summary), full_objective_achieved=False)
(p/'independent-audit.json').write_text(json.dumps({'processes': 36, 'observations': 3456,
    'source_binary_contracts_verified': True, 'windows': summary, 'peer_comparisons': peer, 'decision': decision}, indent=2)+'\n')
print('36 processes / 3456 observations independently reconstructed; failed windows:', sum(not x['passes'] for x in summary))
