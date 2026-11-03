"""Frozen native INSERT gate, with independent A/A and prepared read controls."""
from pathlib import Path
import argparse, hashlib, itertools, json, math, statistics as st, subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--baseline-binary', type=Path, required=True)
parser.add_argument('--candidate-binary', type=Path, required=True)
args = parser.parse_args()
assets = Path(__file__).resolve().parent
plan = json.loads((assets/'plan.json').read_text())
out = args.output.resolve()
out.mkdir(exist_ok=False)
(out/'plan.json').write_text(json.dumps(plan, indent=2)+'\n')
bins = {'baseline': args.baseline_binary.resolve(), 'candidate': args.candidate_binary.resolve()}
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
hashes = {role: sha(binary) for role, binary in bins.items()}
assert hashes['baseline'] != hashes['candidate']
cases = {'insert_native': None, 'point_get': ('docs | id == "d-20" | {id,title}', ['id', 'title']),
         'join_filter': ('orders | total > 100 | join users on user_id | {id,users.email,total} | take all',
                         ['id', 'users.email', 'total'])}
records = []
orders = list(itertools.permutations(['baseline', 'control', 'candidate']))
for round in range(1, 13):
    for n in plan['sizes']:
        users = max(1, n//10)
        expected = {'point_get': [['d-20', 'doc 20 wal note']],
                    'join_filter': [[f'o-{i}', f'u{i%users}@bench.local', i%200]
                                    for i in range(n) if i%200 > 100],
                    'insert_native': [[f'd-{i}', f'bench://{i}', 'rag' if i%2 == 0 else 'sys',
                                       f'doc {i} '+('wal note' if i%10 == 0 else 'plain'),
                                       123456, f'body {i}'] for i in range(n)]}
        for role in orders[(round-1)%6]:
            binary_role = 'baseline' if role == 'control' else role
            binary = bins[binary_role]
            assert sha(binary) == hashes[binary_role]
            prefix = f'{round:02}-{n}-{role}'
            with (out/(prefix+'.stderr')).open('w') as err:
                worker = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                          stderr=err, text=True)
                def ask(request):
                    worker.stdin.write(json.dumps(request)+'\n')
                    worker.stdin.flush()
                    line = worker.stdout.readline()
                    assert line, 'worker exited before response'
                    return json.loads(line)
                try:
                    for case, query in cases.items():
                        if case == 'point_get':
                            assert ask({'rows': n})['ready']
                        if case == 'insert_native':
                            request = {'insert_rows': n, 'samples': plan['native_samples'], 'timestamp_ms': 123456}
                        else:
                            source, fields = query
                            request = {'source': source, 'fields': fields, 'samples': plan['read_samples'],
                                       'iterations': 512 if case == 'point_get' else 4}
                        response = ask(request)
                        if case == 'insert_native':
                            assert response['result'] == expected[case]
                        else:
                            assert sorted(map(tuple, response['result'])) == sorted(map(tuple, expected[case]))
                        assert len(response['samples_ns']) == request['samples']
                        assert all(math.isfinite(x) and x > 0 for x in response['samples_ns'])
                        if case == 'insert_native':
                            # Every value validated before compacting its final readback.
                            value = response.pop('result')
                            response['validated_result_rows'] = len(value)
                            response['result_sha256'] = hashlib.sha256(json.dumps(value, separators=(',', ':')).encode()).hexdigest()
                        name = prefix+'-'+case+'.json'
                        (out/name).write_text(json.dumps({'request': request, 'response': response})+'\n')
                        records.append({'round': round, 'rows': n, 'role': role, 'case': case, 'file': name,
                                        'binary_sha256': hashes[binary_role], 'median_ns': st.median(response['samples_ns'])})
                finally:
                    worker.stdin.close()
                    status = worker.wait()
                    worker.stdout.close()
                    assert status == 0
            (out/'records.json').write_text(json.dumps(records, indent=2)+'\n')
            print(prefix, 'complete', flush=True)
assert len(records) == 324
summary = []
for n in plan['sizes']:
    for case in cases:
        values = {(r['round'], r['role']): r['median_ns'] for r in records if r['rows'] == n and r['case'] == case}
        for series, start in [(1, 1), (2, 7)]:
            gains = [100*(1-values[i, 'candidate']/values[i, 'baseline']) for i in range(start, start+6)]
            aa = [abs(100*(1-values[i, 'control']/values[i, 'baseline'])) for i in range(start, start+6)]
            gain, noise = st.median(gains), st.median(aa)
            target = case == 'insert_native'
            positive = sum(g > 0 for g in gains)
            ok = positive == 6 and gain > noise if target else gain >= -max(2, noise)
            summary.append({'rows': n, 'case': case, 'series': series, 'target': target,
                            'gains_percent': gains, 'aa_absolute_percent': aa, 'median_gain_percent': gain,
                            'median_aa_percent': noise, 'positive_pairs': positive, 'passes': ok})
(out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
(out/'decision.json').write_text(json.dumps({'complete': True, 'processes': 108, 'case_observations': 324,
    'wall_samples': 6048, 'component_gate_passes': all(r['passes'] for r in summary),
    'full_peer_qualified': False, 'full_objective_achieved': False}, indent=2)+'\n')
print('All measurements retained; failed windows:', json.dumps([r for r in summary if not r['passes']]), flush=True)
