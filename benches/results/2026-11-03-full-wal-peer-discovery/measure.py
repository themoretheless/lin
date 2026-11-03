"""Six original FULL WAL peer processes; preserve losses, no numerical retry."""
from pathlib import Path
import hashlib, json, math, os, statistics as st, subprocess, sys

assets = Path(__file__).resolve().parent
spec = json.loads((assets/'input.json').read_text())
out = Path(sys.argv[1]).resolve()
out.mkdir(exist_ok=False)
binary = Path(spec['binary'])
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
def verify():
    assert sha(binary) == spec['binary_sha256']
    for name, digest in spec['source_sha256'].items():
        assert sha(Path(name)) == digest
verify()
expected = {f'compare/durable_insert_wal_{n}/{e}' for n in spec['sizes'] for e in spec['engines']}
medians = {}
status = []
for i in range(1, spec['processes']+1):
    folder = out/f'process-{i}'
    command = [str(binary), '--filter', 'durable_insert_wal', '--samples', str(spec['samples']),
               '--warmup-ms', '0', '--max-iterations', '1', '--output', str(folder)]
    env = dict(os.environ, LIN_BENCH_SKIP_PG='1', LIN_BENCH_SKIP_MYSQL='1')
    with (out/f'process-{i}.log').open('w') as log:
        proc = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    status.append({'process': i, 'exit': proc.returncode, 'command': command, 'binary_sha256': sha(binary)})
    (out/'process-status.json').write_text(json.dumps(status, indent=2)+'\n')
    assert proc.returncode == 0
    d = json.loads((folder/'run.json').read_text())
    assert d['status'] == 'complete' and {c['id'] for c in d['cases']} == expected
    assert len(d['observations']) == 4*spec['samples']
    for case in expected:
        obs = [x for x in d['observations'] if x['case'] == case]
        assert len(obs) == spec['samples']
        values = []
        for x in obs:
            assert x['metric'] == 'wall' and x['operations'] == 1 and x['availability']['state'] == 'available'
            value = float(x['value'])
            assert math.isfinite(value) and value > 0
            values.append(value)
        medians[i, case] = st.median(values)
    verify()
    print('Original process', i, 'complete', flush=True)
summary = []
for n in spec['sizes']:
    lin = [medians[i, f'compare/durable_insert_wal_{n}/lin'] for i in range(1, 7)]
    peer = [medians[i, f'compare/durable_insert_wal_{n}/sqlite'] for i in range(1, 7)]
    ratios = [p/l for l, p in zip(lin, peer)]
    summary.append({'rows': n, 'lin_process_medians_ns': lin, 'sqlite_process_medians_ns': peer,
                    'peer_over_lin_process_ratios': ratios, 'positive_processes': sum(x > 1 for x in ratios),
                    'aggregate_peer_over_lin': st.median(peer)/st.median(lin),
                    'passes_peer_discovery': all(x > 1 for x in ratios) and st.median(peer) > st.median(lin)})
(out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
(out/'decision.json').write_text(json.dumps({'complete': True, 'processes': 6, 'observations': 192,
    'full_wal_local_peer_gate_passes': all(x['passes_peer_discovery'] for x in summary),
    'full_objective_achieved': False}, indent=2)+'\n')
print(json.dumps(summary, indent=2), flush=True)
