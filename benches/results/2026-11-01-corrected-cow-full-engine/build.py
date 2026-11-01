"""Build exact frozen roles in separate Cargo targets; restore all source bytes."""
from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys

p = Path(__file__).resolve().parent
m = json.loads((p / 'source-inputs.json').read_text())
sha = lambda data: hashlib.sha256(data).hexdigest()

def blob(ref, name):
    return subprocess.check_output(['git', 'show', ref + ':' + name])

sources = {}
for role in ['baseline', 'candidate']:
    info = m[role]
    names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', info['commit'], 'src'], text=True).splitlines()
    assert {n for n in names if n.endswith('.rs')} == set(info['source_sha256'])
    sources[role] = {}
    for name, digest in info['source_sha256'].items():
        data = blob(info['commit'], name)
        assert sha(data) == digest, (role, name)
        sources[role][name] = data
    for name, digest in info['build_sha256'].items():
        assert sha(blob(info['commit'], name)) == digest
        assert sha(Path(name).read_bytes()) == digest, name
if sys.argv[1:] == ['--verify-inputs']:
    print('PASS: exact source trees and identical dependency/point-value build inputs')
    raise SystemExit(0)
assert not sys.argv[1:]
# Artifact audit must establish both prerequisite runs, not just a green job badge.
qa = json.loads((p / 'correctness-prerequisites.json').read_text())
for role in ['baseline', 'candidate']:
    assert qa[role]['run_id'] == m['correctness_runs'][role]
    assert qa[role]['source_commit'] == m[role]['commit']
    assert qa[role]['raw_artifact_verified'] is True
    assert qa[role]['qualification_passed'] is True
before = {str(f): f.read_bytes() for f in Path('src').rglob('*.rs')}
assert {n: sha(b) for n, b in before.items()} == m['candidate']['source_sha256']
bench = Path('benches/compare.rs'); bench_before = bench.read_bytes()
harness = (p / 'compare.rs').read_bytes()
status = dict(instrumentation=False, source_sha256=m['candidate']['source_sha256'],
              original_compare_sha256=sha(bench_before), compiled_compare_sha256=sha(harness),
              frozen_commits={r: m[r]['commit'] for r in sources}, binaries={}, source_restored=False)
sys.path.insert(0, str(Path.cwd() / 'scripts'))
from cargo_bench_artifact import bench_executable
try:
    bench.write_bytes(harness)
    for role in ['baseline', 'candidate']:
        for f in Path('src').rglob('*.rs'):
            if str(f) not in sources[role]:
                f.unlink()
        for name, data in sources[role].items():
            Path(name).parent.mkdir(parents=True, exist_ok=True)
            Path(name).write_bytes(data)
        target = Path('target') / ('corrected-full-engine-' + role)
        env = dict(os.environ, CARGO_TARGET_DIR=str(target))
        with (p / ('build-' + role + '.log')).open('w') as log:
            result = subprocess.run(['cargo', 'bench', '--bench', 'compare', '--no-run', '--color', 'never'], env=env, stdout=log, stderr=subprocess.STDOUT)
        assert result.returncode == 0, role
        executable = Path(bench_executable((p / ('build-' + role + '.log')).read_text(), 'compare')).resolve()
        assert executable.is_relative_to(target.resolve()), executable
        dest = Path('/tmp') / ('lin-corrected-full-engine-' + role)
        shutil.copy2(executable, dest)
        status['binaries'][role] = dict(path=str(dest), sha256=sha(dest.read_bytes()), cargo_target_dir=str(target), source_sha256=m[role]['source_sha256'])
finally:
    for f in Path('src').rglob('*.rs'):
        if str(f) not in before:
            f.unlink()
    for name, data in before.items():
        Path(name).write_bytes(data)
    bench.write_bytes(bench_before)
    status['source_restored'] = {str(f): sha(f.read_bytes()) for f in Path('src').rglob('*.rs')} == m['candidate']['source_sha256'] and bench.read_bytes() == bench_before
    (p / 'provenance.json').write_text(json.dumps(status, indent=2) + '\n')
assert status['source_restored'] and len(status['binaries']) == 2
