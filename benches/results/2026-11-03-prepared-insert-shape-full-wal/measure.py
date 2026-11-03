"""Frozen prepared INSERT shape FULL WAL gate; all original processes retained."""
from pathlib import Path
import hashlib, itertools, json, math, os, statistics as st, subprocess, sys

assets = Path(__file__).resolve().parent
spec = json.loads((assets/'input.json').read_text())
out = Path(sys.argv[1]).resolve()
# Refuse before creating an output directory or starting any measured process.
logical_cpus = os.cpu_count() or 1
initial_load = os.getloadavg()
assert initial_load[0] <= logical_cpus * 0.75, ('Host busy; no timings executed', initial_load, logical_cpus)
out.mkdir(exist_ok=False)
(out/'host-start.json').write_text(json.dumps({'logical_cpus': logical_cpus, 'load_average': initial_load})+'\n')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
def verify():
    for role, info in spec['binaries'].items():
        assert sha(Path(info['path'])) == info['sha256'], role
    for name, digest in spec['current_source_sha256'].items():
        assert sha(Path(name)) == digest, name
    assert sha(Path(__file__)) == spec['runner_sha256']
verify()
cases = {f'compare/durable_insert_wal_{n}/{engine}' for n in ['1k', '10k'] for engine in ['lin', 'sqlite']}
orders = list(itertools.permutations(['baseline', 'control', 'candidate']))
records = []
medians = {}
for round in range(1, 13):
    for role in orders[(round-1)%6]:
        info = spec['binaries']['baseline' if role == 'control' else role]
        folder = out/f'{round:02}-{role}'
        cmd = [info['path'], '--filter', 'durable_insert_wal', '--samples', '24',
               '--warmup-ms', '0', '--max-iterations', '1', '--output', str(folder)]
        with (out/f'{round:02}-{role}.log').open('w') as log:
            proc = subprocess.run(cmd, env=dict(os.environ, LIN_BENCH_SKIP_PG='1', LIN_BENCH_SKIP_MYSQL='1'),
                                  stdout=log, stderr=subprocess.STDOUT)
        record = dict(round=round, role=role, exit=proc.returncode, binary_sha256=sha(Path(info['path'])), command=cmd, load_average_after=list(os.getloadavg()))
        records.append(record)
        (out/'process-status.json').write_text(json.dumps(records, indent=2)+'\n')
        assert proc.returncode == 0
        d = json.loads((folder/'run.json').read_text())
        assert d['status'] == 'complete' and {c['id'] for c in d['cases']} == cases
        assert len(d['observations']) == 96
        for c in d['cases']:
            assert c['contract']['validation'] == 'fresh operation before calibration and after measurement'
        for case in cases:
            obs = [x for x in d['observations'] if x['case'] == case]
            assert len(obs) == 24
            vals = []
            for x in obs:
                assert x['metric'] == 'wall' and x['operations'] == 1 and x['availability']['state'] == 'available'
                value = float(x['value'])
                assert math.isfinite(value) and value > 0
                vals.append(value)
            medians[round, role, case] = st.median(vals)
        verify()
        print(round, role, 'complete', flush=True)
assert len(records) == 36
summary = []
for case in sorted(cases):
    for series, start in [(1, 1), (2, 7)]:
        gains = [100*(1-medians[i, 'candidate', case]/medians[i, 'baseline', case]) for i in range(start, start+6)]
        aa = [abs(100*(1-medians[i, 'control', case]/medians[i, 'baseline', case])) for i in range(start, start+6)]
        gain, noise = st.median(gains), st.median(aa)
        target = case == 'compare/durable_insert_wal_1k/lin'
        positive = sum(x > 0 for x in gains)
        ok = positive == 6 and gain > noise if target else gain >= -max(2, noise)
        summary.append(dict(case=case, series=series, target=target, gains_percent=gains,
                            aa_absolute_percent=aa, median_gain_percent=gain, median_aa_percent=noise,
                            positive_pairs=positive, passes=ok))
peer = []
for n in ['1k', '10k']:
    ratios = [medians[i, 'candidate', f'compare/durable_insert_wal_{n}/sqlite'] /
              medians[i, 'candidate', f'compare/durable_insert_wal_{n}/lin'] for i in range(1, 13)]
    peer.append(dict(rows=n, sqlite_over_lin_process_ratios=ratios, positive_processes=sum(x > 1 for x in ratios)))
(out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
(out/'peer-comparisons.json').write_text(json.dumps(peer, indent=2)+'\n')
(out/'decision.json').write_text(json.dumps({'complete': True, 'processes': 36, 'observations': 3456,
    'component_gate_passes': all(x['passes'] for x in summary), 'full_objective_achieved': False}, indent=2)+'\n')
print('Failed frozen windows:', json.dumps([x for x in summary if not x['passes']]), flush=True)
