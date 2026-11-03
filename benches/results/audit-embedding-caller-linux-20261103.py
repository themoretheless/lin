"""Audit the frozen caller-embedding Linux gate; never executes binaries or timings."""
from pathlib import Path
import argparse, hashlib, json, math, re, statistics as st, subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--raw', type=Path, required=True)
parser.add_argument('--workspace-log', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
p = args.raw
sha = lambda data: hashlib.sha256(data).hexdigest()
load = lambda path: json.loads(path.read_text())
def blob(ref, name):
    return subprocess.check_output(['git', 'show', ref+':'+name], cwd=root)
meta = load(p/'build-provenance.json')
assert meta['status'] == 'finished' and meta['measurement_exit'] == 0 and meta['source_unchanged']
head = meta['head']
slug = 'benches/results/2026-11-03-embedding-caller-linux/'
spec = json.loads(blob(head, slug+'input.json'))
plan = json.loads(blob(head, slug+'plan.json'))
for name, digest in spec['files_sha256'].items():
    assert sha(blob(head, name)) == digest
assert plan == load(p/'measurement/plan.json')
assert sorted(spec['runtime_files']) == sorted(spec['rust_source_files']+[
    'Cargo.toml', 'Cargo.lock', 'lin-derive/Cargo.toml', 'tools/peer-worker/Cargo.toml'])
assert sorted(meta['builds']) == ['baseline', 'candidate']
for role, build in meta['builds'].items():
    ref = head if role == 'candidate' else spec['baseline_reference']
    assert build['reference'] == ref
    assert set(build['source_sha256']) == set(spec['runtime_files'])
    for name, digest in build['source_sha256'].items():
        assert sha(blob(ref, name)) == digest
    binary = (p/f'{role}-worker').read_bytes()
    assert binary[:6] == b'\x7fELF\x02\x01' and int.from_bytes(binary[18:20], 'little') == 62
    assert sha(binary) == build['binary_sha256']
assert [name for name in spec['runtime_files'] if meta['builds']['candidate']['source_sha256'][name]
        != meta['builds']['baseline']['source_sha256'][name]] == ['src/embed.rs']
log = args.workspace_log.read_text()
assert 'test embed::tests::parallel_hashing_batches_preserve_order_and_exact_vector_bits ... ok' in log
assert not re.search(r'test result: FAILED|could not compile|error\[E', log)
totals = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
assert totals and all(int(x[1]) == 0 for x in totals)
test_counts = [sum(int(row[i]) for row in totals) for i in range(3)]
assert test_counts[0] >= 400
records = load(p/'measurement/records.json')
assert len(records) == 324
cases = ['insert_native', 'point_get', 'join_filter']
coords = set()
medians = {}
samples = 0
for record in records:
    r, n, role, case = (record[k] for k in ['round', 'rows', 'role', 'case'])
    coordinate = (r, n, role, case)
    assert coordinate not in coords
    coords.add(coordinate)
    assert record['file'] == f'{r:02}-{n}-{role}-{case}.json'
    binary_role = 'baseline' if role == 'control' else role
    assert record['binary_sha256'] == meta['builds'][binary_role]['binary_sha256']
    data = load(p/'measurement'/record['file'])
    req, response = data['request'], data['response']
    values = response['samples_ns']
    want_samples = plan['native_samples'] if case == 'insert_native' else plan['read_samples']
    assert len(values) == want_samples and req['samples'] == want_samples
    assert all(type(x) in [int, float] and math.isfinite(x) and x > 0 for x in values)
    value = st.median(values)
    assert record['median_ns'] == value
    medians[coordinate] = value
    samples += len(values)
    users = max(1, n//10)
    if case == 'insert_native':
        assert req == {'insert_rows': n, 'samples': want_samples, 'timestamp_ms': 123456}
        expected = [[f'd-{i}', f'bench://{i}', 'rag' if i%2 == 0 else 'sys',
                     f'doc {i} '+('wal note' if i%10 == 0 else 'plain'), 123456, f'body {i}'] for i in range(n)]
        assert response['validated_result_rows'] == n
        assert response['result_sha256'] == sha(json.dumps(expected, separators=(',', ':')).encode())
        assert response['statement_rows_limit'] == 10000 and response['statements'] == max(1, n//10000)
        assert response['api'] == ('prepared run' if n <= 10000 else 'prepared chunks in one memory transaction')
    else:
        if case == 'point_get':
            source, fields, iterations = 'docs | id == "d-20" | {id,title}', ['id', 'title'], 512
            expected = [['d-20', 'doc 20 wal note']]
        else:
            source = 'orders | total > 100 | join users on user_id | {id,users.email,total} | take all'
            fields, iterations = ['id', 'users.email', 'total'], 4
            expected = [[f'o-{i}', f'u{i%users}@bench.local', i%200] for i in range(n) if i%200 > 100]
        assert req == dict(source=source, fields=fields, iterations=iterations, samples=want_samples)
        assert sorted(map(tuple, response['result'])) == sorted(map(tuple, expected))
assert coords == {(r, n, role, case) for r in range(1, 13) for n in plan['sizes']
                  for role in ['baseline', 'control', 'candidate'] for case in cases}
assert samples == 6048
assert {f.name for f in (p/'measurement').glob('*.stderr')} == {
    f'{r:02}-{n}-{role}.stderr' for r in range(1, 13) for n in plan['sizes'] for role in ['baseline', 'control', 'candidate']}
assert all(not f.read_text() for f in (p/'measurement').glob('*.stderr'))
summary = []
for n in plan['sizes']:
    for case in cases:
        for series, start in [(1, 1), (2, 7)]:
            gains = [100*(1-medians[i, n, 'candidate', case]/medians[i, n, 'baseline', case]) for i in range(start, start+6)]
            aa = [abs(100*(1-medians[i, n, 'control', case]/medians[i, n, 'baseline', case])) for i in range(start, start+6)]
            gain, noise = st.median(gains), st.median(aa)
            target = case == 'insert_native'
            positive = sum(x > 0 for x in gains)
            ok = positive == 6 and gain > noise if target else gain >= -max(2, noise)
            summary.append(dict(rows=n, case=case, series=series, target=target, gains_percent=gains,
                                aa_absolute_percent=aa, median_gain_percent=gain, median_aa_percent=noise,
                                positive_pairs=positive, passes=ok))
assert summary == load(p/'measurement/summary.json')
decision = load(p/'measurement/decision.json')
assert decision == dict(complete=True, processes=108, case_observations=324, wall_samples=6048,
                       component_gate_passes=all(r['passes'] for r in summary),
                       full_peer_qualified=False, full_objective_achieved=False)
args.output.write_text(json.dumps({'source_head': head, 'decision': decision, 'workspace_counts': test_counts,
    'source_binaries_values_verified': True, 'windows': summary}, indent=2)+'\n')
print('108 processes / 324 observations / 6048 samples reconstructed; failed windows:', sum(not r['passes'] for r in summary))
