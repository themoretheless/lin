"""Reconstruct downloaded diagnostic evidence without executing its binaries."""
from pathlib import Path
import argparse, hashlib, json, math, re, statistics, subprocess, tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('artifact', type=Path)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--expected-head', required=True)
args = parser.parse_args()
p = args.artifact
sha = lambda b: hashlib.sha256(b).hexdigest()
load = lambda f: json.loads(f.read_text())
m = load(p/'provenance.json')
assert m['head'] == args.expected_head
assert m['status'] == 'finished' and m['source_unchanged'] and m['source_restored']
assert m['diagnostic_only'] and not m['full_objective_achieved']
harness = 'benches/results/2026-11-03-vector-finish-linux/profile.py'
assert sha(subprocess.check_output(['git','show',m['head']+':'+harness])) == m['harness_sha256']
assert m['platform'].startswith('Linux-') and m['cpu_count'] >= 1 and m['affinity']
manifest = load(p/'manifest.json')
assert set(manifest) == {f.name for f in p.iterdir() if f.is_file()} - {'manifest.json'}
for name, info in manifest.items():
    assert Path(name).name == name
    data = (p/name).read_bytes()
    assert len(data) == info['bytes'] and sha(data) == info['sha256']
for name, digest in m['source_sha256'].items():
    actual = subprocess.check_output(['git', 'show', m['head']+':'+name])
    baseline = subprocess.check_output(['git','show','b9f10bc776b05eb5235237e9a900c2ef6aaed1fd:'+name])
    assert sha(actual) == digest and actual == baseline
for name, digest in m['patches'].items():
    assert sha(subprocess.check_output(['git', 'show', m['head']+':'+name])) == digest
assert m['builds']['ordinary']['source_sha256'] == m['source_sha256']
assert m['builds']['diagnostic']['source_sha256'] == m['instrumented_source_sha256']
assert set(m['instrumented_source_sha256']) == set(m['source_sha256'])
assert [k for k in m['source_sha256'] if m['source_sha256'][k] != m['instrumented_source_sha256'][k]] == ['src/embed.rs']
assert set(m['patches']) == {'benches/results/2026-11-03-vector-finish-linux/instrumentation.patch'}
patch_name = next(iter(m['patches']))
patch = subprocess.check_output(['git','show',m['head']+':'+patch_name])
with tempfile.TemporaryDirectory(prefix='lin-embed-audit-') as temp:
    temp = Path(temp)
    (temp/'src').mkdir()
    original = subprocess.check_output(['git','show',m['head']+':src/embed.rs'])
    (temp/'src/embed.rs').write_bytes(original)
    subprocess.run(['git','apply','--check','-'], input=patch, cwd=temp, check=True)
    subprocess.run(['git','apply','-'], input=patch, cwd=temp, check=True)
    assert sha((temp/'src/embed.rs').read_bytes()) == m['instrumented_source_sha256']['src/embed.rs']
for role in ['ordinary', 'diagnostic']:
    binary = (p/f'{role}-worker').read_bytes()
    assert binary[:4] == b'\x7fELF' and binary[4] == 2 and binary[5] == 1
    assert int.from_bytes(binary[18:20], 'little') == 62  # ELF x86_64
    assert sha(binary) == m['builds'][role]['sha256']
    build_log=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', (p/f'build-{role}.log').read_text())
    assert re.search(r'Compiling lin v',build_log) and re.search(r'Compiling lin-peer-worker v',build_log)
    assert (p/f'clean-{role}.log').exists()
assert len(m['runs']) == 6
seen = set()
report = []
for run in m['runs']:
    n, role = run['rows'], run['role']
    assert (n, role) not in seen
    seen.add((n, role))
    d = load(p/f'{role}-{n}.json')
    expected = [[f'd-{i}', f'bench://{i}', 'rag' if i % 2 == 0 else 'sys',
                 f'doc {i} '+('wal note' if i % 10 == 0 else 'plain'),
                 123456, f'body {i}'] for i in range(n)]
    assert d['result'] == expected
    assert len(d['samples_ns']) == 3 and all(math.isfinite(x) and x > 0 for x in d['samples_ns'])
    assert run['wall_samples_ns'] == d['samples_ns']
    assert run['binary_sha256'] == m['builds'][role]['sha256']
    item = {'rows': n, 'role': role, 'median_wall_ms': statistics.median(d['samples_ns'])/1e6}
    lines = (p/f'{role}-{n}.stderr').read_text().splitlines()
    if role == 'ordinary':
        assert not lines
    else:
        observations = []
        for line in lines:
            match = re.fullmatch(r'FINISH_DETAIL rows=(\d+) normalize_ns=(\d+) arc_ns=(\d+) cleanup_ns=(\d+)',line)
            assert match, line
            rows,normalize_ns,arc_ns,cleanup_ns=map(int,match.groups())
            assert 0 < rows <= min(n,10000) and min(normalize_ns,arc_ns,cleanup_ns)>0
            observations.append(dict(rows=rows,normalize_ns=normalize_ns,arc_ns=arc_ns,cleanup_ns=cleanup_ns))
        assert sum(o['rows'] for o in observations) == 3*n
        assert observations == run['embedding_observations']
        totals={key:sum(o[key] for o in observations) for key in ['normalize_ns','arc_ns','cleanup_ns']}
        assert totals == run['aggregate_worker_cpu_ns']
        item['worker_accumulated_ns_per_row']={key:value/(3*n) for key,value in totals.items()}
        item['chunk_observations']=len(observations)
    report.append(item)
assert seen == {(n, role) for n in [1000, 10000, 100000] for role in ['ordinary', 'diagnostic']}
args.output.write_text(json.dumps({'source_head': m['head'], 'processes': 6, 'wall_samples': 18,
    'source_binary_values_verified': True, 'results': report, 'diagnostic_only': True,
    'interpretation': 'Per-worker durations overlap; instrumentation adds per-text timers. No wall-time fractions or speedup claim.',
    'full_objective_achieved': False}, indent=2)+'\n')
print('Six processes / 18 wall samples, exact values and both source/binary closures verified')
