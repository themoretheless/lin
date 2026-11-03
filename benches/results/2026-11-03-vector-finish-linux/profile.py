"""Diagnostic hashing-vector breakdown; no candidate adoption or peer timing retry."""
from pathlib import Path
import hashlib, json, math, os, platform, re, shutil, statistics, subprocess, sys

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
    'cpu_count': os.cpu_count(), 'affinity': sorted(os.sched_getaffinity(0)),
    'harness_sha256': sha(Path(__file__).resolve()),
    'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
    'diagnostic_only': True, 'full_objective_achieved': False, 'builds': {}, 'runs': []}
def save():
    (out/'provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
def build(role):
    with (out/f'clean-{role}.log').open('w') as log:
        subprocess.run(['cargo', '--color', 'never', 'clean', '--release', '-p', 'lin', '-p', 'lin-peer-worker'], cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
    with (out/f'build-{role}.log').open('w') as log:
        subprocess.run(['cargo', '--color', 'never', 'build', '--locked', '--release', '--verbose', '-p', 'lin-peer-worker'],
                       cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
    log_text = (out/f'build-{role}.log').read_text()
    assert re.search(r'Compiling lin v', log_text) and re.search(r'Compiling lin-peer-worker v', log_text)
    dest = out/f'{role}-worker'
    shutil.copy2(root/'target/release/lin-peer-worker', dest)
    assert dest.read_bytes()[:4] == b'\x7fELF'
    metadata['builds'][role] = {'sha256': sha(dest), 'source_sha256': closure()}
    save()
    return dest
save()
original = (root/'src/embed.rs').read_bytes()
try:
    ordinary = build('ordinary')
    patches = [root/'benches/results/2026-11-03-vector-finish-linux/instrumentation.patch']
    metadata['patches'] = {str(p.relative_to(root)): sha(p) for p in patches}
    for patch in patches:
        subprocess.run(['git', 'apply', '--check', str(patch)], cwd=root, check=True)
        subprocess.run(['git', 'apply', str(patch)], cwd=root, check=True)
    metadata['instrumented_source_sha256'] = closure()
    diagnostic = build('diagnostic')
finally:
    (root/'src/embed.rs').write_bytes(original)
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
        lines = proc.stderr.splitlines()
        if role == 'ordinary':
            assert not lines, lines
        else:
            observations = []
            for line in lines:
                match = re.fullmatch(r'FINISH_DETAIL rows=(\d+) normalize_ns=(\d+) arc_ns=(\d+) cleanup_ns=(\d+)', line)
                assert match, line
                rows, normalize_ns, arc_ns, cleanup_ns = map(int, match.groups())
                assert rows > 0 and normalize_ns > 0 and arc_ns > 0 and cleanup_ns > 0
                observations.append(dict(rows=rows, normalize_ns=normalize_ns, arc_ns=arc_ns, cleanup_ns=cleanup_ns))
            assert sum(o['rows'] for o in observations) == 3*n
            item['embedding_observations'] = observations
            item['aggregate_worker_cpu_ns'] = {k:sum(o[k] for o in observations) for k in ['normalize_ns','arc_ns','cleanup_ns']}
        metadata['runs'].append(item)
        save()
assert closure() == source and len(metadata['runs']) == 6
metadata['status'] = 'finished'
metadata['source_unchanged'] = True
save()
(out/'manifest.json').write_text(json.dumps({p.name: {'bytes': p.stat().st_size, 'sha256': sha(p)}
    for p in sorted(out.iterdir()) if p.is_file()}, indent=2)+'\n')
print('Hashing-vector profile finished: six processes, 18 wall samples; diagnostic worker CPU attribution only')
