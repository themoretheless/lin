"""Diagnostic Linux phase attribution; no candidate adoption or peer timing retry."""
from pathlib import Path
import hashlib, json, math, platform, re, shutil, statistics, subprocess, sys

root = Path(__file__).resolve().parents[3]
out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=False)
assert platform.machine() == 'x86_64' and platform.system() == 'Linux'
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def closure():
    files = [root/'Cargo.toml', root/'Cargo.lock', root/'lin-derive/Cargo.toml',
             root/'tools/peer-worker/Cargo.toml']
    for name in ['src', 'lin-derive/src', 'tools/peer-worker/src']:
        files.extend(sorted((root/name).rglob('*.rs')))
    return {str(p.relative_to(root)): sha(p) for p in files}
source = closure()
metadata = {'status': 'building', 'head': subprocess.check_output(
    ['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    'platform': platform.platform(), 'source_sha256': source,
    'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
    'diagnostic_only': True, 'full_objective_achieved': False, 'builds': {}, 'runs': []}
def save():
    (out/'provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
def build(role):
    with (out/f'build-{role}.log').open('w') as log:
        subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'lin-peer-worker'],
                       cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
    dest = out/f'{role}-worker'
    shutil.copy2(root/'target/release/lin-peer-worker', dest)
    assert dest.read_bytes()[:4] == b'\x7fELF'
    metadata['builds'][role] = {'sha256': sha(dest), 'source_sha256': closure()}
    save()
    return dest
save()
original = (root/'src/exec.rs').read_bytes()
try:
    ordinary = build('ordinary')
    patches = [root/f'benches/results/{slug}/instrumentation.patch' for slug in
               ['2026-11-03-insert-phase-discovery', '2026-11-03-insert-detailed-discovery']]
    metadata['patches'] = {str(p.relative_to(root)): sha(p) for p in patches}
    for patch in patches:
        subprocess.run(['git', 'apply', '--check', str(patch)], cwd=root, check=True)
        subprocess.run(['git', 'apply', str(patch)], cwd=root, check=True)
    metadata['instrumented_source_sha256'] = closure()
    diagnostic = build('diagnostic')
finally:
    (root/'src/exec.rs').write_bytes(original)
    metadata['source_restored'] = closure() == source
    save()
assert metadata['source_restored']
metadata['status'] = 'measuring'
save()
for n in [1000, 10000, 100000]:
    for role, binary in [('ordinary', ordinary), ('diagnostic', diagnostic)]:
        request = {'insert_rows': n, 'samples': 3, 'timestamp_ms': 123456}
        proc = subprocess.run([str(binary)], input=json.dumps(request)+'\n',
                              text=True, capture_output=True)
        (out/f'{role}-{n}.stderr').write_text(proc.stderr)
        assert proc.returncode == 0
        data = json.loads(proc.stdout)
        expected = [[f'd-{i}', f'bench://{i}', 'rag' if i % 2 == 0 else 'sys',
                     f'doc {i} '+('wal note' if i % 10 == 0 else 'plain'),
                     123456, f'body {i}'] for i in range(n)]
        assert data['result'] == expected
        assert len(data['samples_ns']) == 3 and all(math.isfinite(v) and v > 0 for v in data['samples_ns'])
        # Preserve complete values for independent replay, outside timing.
        (out/f'{role}-{n}.json').write_text(proc.stdout)
        item = {'rows': n, 'role': role, 'wall_samples_ns': data['samples_ns'],
                'binary_sha256': sha(binary), 'exact_values_passed': True}
        if role == 'diagnostic':
            lines = proc.stderr.splitlines()
            grouped = {}
            for line in lines:
                match = re.fullmatch(r'(INSERT_PHASE|DETAIL) (\w+) rows=(\d+) us=(\d+)', line)
                assert match, line
                kind, phase, rows, value = match.groups()
                assert int(rows) == min(n, 10000)
                grouped.setdefault(kind+'/'+phase, []).append(int(value))
            expected_count = 3 * max(1, n//10000)
            assert set(grouped) == {'INSERT_PHASE/'+k for k in ['build', 'validate', 'embed', 'maintain']} | {
                'DETAIL/'+k for k in ['text', 'vectors', 'attach', 'reserve', 'index', 'fts', 'hnsw', 'maps']}
            assert all(len(values) == expected_count for values in grouped.values())
            item['phase_median_us'] = {key: statistics.median(values) for key, values in grouped.items()}
            item['phase_observations'] = grouped
            # Non-overlapping coarse phases; residual includes all work outside
            # these scopes plus phase-boundary diagnostic output overhead.
            coarse_us = sum(sum(grouped['INSERT_PHASE/'+k]) for k in ['build', 'validate', 'embed', 'maintain'])
            item['coarse_phase_total_us'] = coarse_us
            item['wall_minus_coarse_total_us'] = sum(data['samples_ns'])/1000-coarse_us
        metadata['runs'].append(item)
        save()
assert closure() == source and len(metadata['runs']) == 6
metadata['status'] = 'finished'
metadata['source_unchanged'] = True
save()
(out/'manifest.json').write_text(json.dumps({p.name: {'bytes': p.stat().st_size, 'sha256': sha(p)}
    for p in sorted(out.iterdir()) if p.is_file()}, indent=2)+'\n')
print('Linux phase profile finished: six processes, 18 wall samples; diagnostic attribution only')
