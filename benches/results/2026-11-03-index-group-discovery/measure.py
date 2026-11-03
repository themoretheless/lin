"""Local discovery, never rerun an unchanged failed numerical candidate."""
from pathlib import Path
import hashlib, json, statistics as st, subprocess, sys

p = Path(__file__).resolve().parent
spec = json.loads((p / 'input.json').read_text())
raw = Path(sys.argv[1])
raw.mkdir(parents=True, exist_ok=False)
for info in spec['binaries'].values():
    assert hashlib.sha256(Path(info['path']).read_bytes()).hexdigest() == info['sha256']
summary = []
for n in spec['rows']:
    medians = {}
    for pair in range(spec['pairs']):
        # Reverse the three roles on alternating pairs, keeping independent A/A.
        roles = ['baseline', 'control', 'candidate']
        if pair % 2:
            roles.reverse()
        for role in roles:
            binary = spec['binaries']['baseline' if role == 'control' else role]
            request = {'insert_rows': n, 'samples': spec['samples'], 'timestamp_ms': 123456}
            proc = subprocess.run([binary['path']], input=json.dumps(request)+'\n',
                                  text=True, capture_output=True)
            stem = raw / f'{n}-{pair}-{role}'
            stem.with_suffix('.stderr').write_text(proc.stderr)
            assert proc.returncode == 0
            d = json.loads(proc.stdout)
            assert len(d['samples_ns']) == spec['samples']
            assert all(x > 0 for x in d['samples_ns'])
            # Independently validate all six fields, in source order.
            expected = [[f'd-{i}', f'bench://{i}', 'rag' if i % 2 == 0 else 'sys',
                         f'doc {i} '+('wal note' if i % 10 == 0 else 'plain'),
                         123456, f'body {i}'] for i in range(n)]
            assert d['result'] == expected
            stem.with_suffix('.json').write_text(proc.stdout)
            medians[pair, role] = st.median(d['samples_ns'])
    gain = [100*(1-medians[i, 'candidate']/medians[i, 'baseline']) for i in range(spec['pairs'])]
    aa = [abs(100*(1-medians[i, 'control']/medians[i, 'baseline'])) for i in range(spec['pairs'])]
    summary.append({'rows': n, 'paired_gains_percent': gain, 'aa_absolute_percent': aa,
                    'median_gain_percent': st.median(gain), 'median_aa_percent': st.median(aa),
                    'passes_discovery': all(x > 0 for x in gain) and st.median(gain) > st.median(aa)})
    print(json.dumps(summary[-1]), flush=True)
(raw/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
