"""Reconstruct downloaded diagnostic evidence without executing its binaries."""
from pathlib import Path
import argparse, hashlib, json, math, re, statistics, subprocess, tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('artifact', type=Path)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
p = args.artifact
sha = lambda b: hashlib.sha256(b).hexdigest()
load = lambda f: json.loads(f.read_text())
m = load(p/'provenance.json')
assert m['status'] == 'finished' and m['source_unchanged'] and m['source_restored']
assert m['diagnostic_only'] and not m['full_objective_achieved']
manifest = load(p/'manifest.json')
assert set(manifest) == {f.name for f in p.iterdir() if f.is_file()} - {'manifest.json'}
for name, info in manifest.items():
    assert Path(name).name == name
    data = (p/name).read_bytes()
    assert len(data) == info['bytes'] and sha(data) == info['sha256']
for name, digest in m['source_sha256'].items():
    assert sha(subprocess.check_output(['git', 'show', m['head']+':'+name])) == digest
assert m['head'] == 'd8871eb36b119b9960989bf4a5feea9877168cc5'
harness_head = '0880db8acc81e65c82482563c82be64ce6ba0d0b'
patch_sources = {
    'instrumentation.patch': (harness_head, 'benches/results/2026-11-03-prepared-shape-phase-profile/instrumentation.patch'),
    'benches/results/2026-11-03-insert-detailed-discovery/instrumentation.patch': (m['head'], 'benches/results/2026-11-03-insert-detailed-discovery/instrumentation.patch')}
assert set(m['patches']) == set(patch_sources)
with tempfile.TemporaryDirectory() as tmp:
    q = Path(tmp)
    (q/'src').mkdir()
    (q/'src/exec.rs').write_bytes(subprocess.check_output(['git', 'show', m['head']+':src/exec.rs']))
    subprocess.run(['git', 'init', '-q', str(q)], check=True)
    for name, (ref, source) in patch_sources.items():
        data = subprocess.check_output(['git', 'show', ref+':'+source])
        assert sha(data) == m['patches'][name]
        subprocess.run(['git', 'apply', '-'], input=data, cwd=q, check=True)
    assert sha((q/'src/exec.rs').read_bytes()) == m['instrumented_source_sha256']['src/exec.rs']
assert m['builds']['ordinary']['source_sha256'] == m['source_sha256']
assert m['builds']['diagnostic']['source_sha256'] == m['instrumented_source_sha256']
assert set(m['instrumented_source_sha256']) == set(m['source_sha256'])
assert [k for k in m['source_sha256'] if m['source_sha256'][k] != m['instrumented_source_sha256'][k]] == ['src/exec.rs']
for role in ['ordinary', 'diagnostic']:
    binary = (p/f'{role}-worker').read_bytes()
    assert binary[:4] == b'\x7fELF' and binary[4] == 2 and binary[5] == 1
    assert int.from_bytes(binary[18:20], 'little') == 62  # ELF x86_64
    assert sha(binary) == m['builds'][role]['sha256']
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
        grouped = {}
        for line in lines:
            match = re.fullmatch(r'(INSERT_PHASE|DETAIL) (\w+) rows=(\d+) us=(\d+)', line)
            assert match
            kind, phase, rows, value = match.groups()
            assert int(rows) == min(n, 10000)
            grouped.setdefault(kind+'/'+phase, []).append(int(value))
        assert set(grouped) == {'INSERT_PHASE/'+k for k in ['build', 'validate', 'embed', 'maintain']} | {'DETAIL/'+k for k in ['text', 'vectors', 'attach', 'reserve', 'index', 'fts', 'hnsw', 'maps']}
        assert grouped == run['phase_observations']
        assert all(len(values) == 3*max(1, n//10000) for values in grouped.values())
        medians = {k: statistics.median(v) for k, v in grouped.items()}
        assert medians == run['phase_median_us']
        total = sum(sum(grouped['INSERT_PHASE/'+k]) for k in ['build', 'validate', 'embed', 'maintain'])
        residual = sum(d['samples_ns'])/1000-total
        assert total == run['coarse_phase_total_us'] and residual == run['wall_minus_coarse_total_us']
        item.update(phase_median_us=medians, residual_total_us=residual)
    report.append(item)
assert seen == {(n, role) for n in [1000, 10000, 100000] for role in ['ordinary', 'diagnostic']}
args.output.write_text(json.dumps({'source_head': m['head'], 'processes': 6, 'wall_samples': 18,
    'source_binary_values_verified': True, 'results': report, 'diagnostic_only': True,
    'full_objective_achieved': False}, indent=2)+'\n')
print('Six processes / 18 wall samples, exact values and both source/binary closures verified')
