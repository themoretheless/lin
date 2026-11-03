from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys, platform, re

assets = Path(__file__).resolve().parent
root = assets.parents[2]
out = Path('/tmp/lin-orders-lifecycle-linux')
out.mkdir(exist_ok=False)
spec = json.loads((assets/'input.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
def verify():
    for name, digest in spec['files_sha256'].items():
        assert sha(root/name) == digest, name
    assert sorted(str(p.relative_to(root)) for base in ['src', 'lin-derive/src', 'tools/peer-worker/src']
                  for p in (root/base).rglob('*.rs')) == spec['rust_source_files']
def closure(path):
    names = spec['runtime_files']
    return {name: sha(path/name) for name in names}
verify()
assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
metadata = {'status': 'building', 'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            'compiler': subprocess.check_output(['rustc', '-Vv'], text=True), 'builds': {}}
def optional_text(name):
    try:
        return Path(name).read_text().strip()
    except OSError:
        return None
cpuinfo = optional_text('/proc/cpuinfo') or ''
metadata['host'] = {'platform': platform.platform(), 'cpu_count': os.cpu_count(),
    'affinity': sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else None,
    'cpu_model': next((line.split(':', 1)[1].strip() for line in cpuinfo.splitlines() if line.startswith('model name')), None),
    'cgroup_cpu_max': optional_text('/sys/fs/cgroup/cpu.max'),
    'cgroup_memory_max': optional_text('/sys/fs/cgroup/memory.max')}
bins = {}
for role in ['candidate', 'baseline']:
    ref = metadata['head'] if role == 'candidate' else spec['baseline_reference']
    path = root if role == 'candidate' else Path('/tmp/lin-orders-lifecycle-baseline-source')
    if role == 'baseline':
        subprocess.run(['git', 'worktree', 'add', '--detach', str(path), ref], check=True)
    source = closure(path)
    if role == 'baseline':
        candidate = metadata['builds']['candidate']['source_sha256']
        assert [name for name in source if source[name] != candidate[name]] == ['src/exec.rs', 'src/store.rs']
    env = dict(os.environ, CARGO_TARGET_DIR=str(root/'target'), CARGO_BUILD_JOBS='2')
    with (out/f'clean-{role}.log').open('w') as log:
        subprocess.run(['cargo', '--color', 'never', 'clean', '--release', '-p', 'lin', '-p', 'lin-peer-worker'], cwd=path, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    with (out/f'build-{role}.log').open('w') as log:
        subprocess.run(['cargo', '--color', 'never', 'build', '--verbose', '--locked', '--release', '-p', 'lin-peer-worker'], cwd=path,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    build_log = re.sub(r'\x1b\[[0-9;]*m', '', (out/f'build-{role}.log').read_text())
    assert re.search(r'Compiling lin v[0-9]', build_log) and re.search(r'Compiling lin-peer-worker v[0-9]', build_log)
    binary = out/f'{role}-worker'
    shutil.copy2(root/'target/release/lin-peer-worker', binary)
    assert source == closure(path)
    assert binary.read_bytes()[:6] == b'\x7fELF\x02\x01'
    bins[role] = binary
    metadata['builds'][role] = {'reference': ref, 'source_sha256': source, 'binary_sha256': sha(binary)}
    (out/'build-provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
    verify()
with (out/'measurement.log').open('w') as log:
    proc = subprocess.run([sys.executable, str(assets/'run.py'), '--output', str(out/'measurement'),
                           '--baseline-binary', str(bins['baseline']), '--candidate-binary', str(bins['candidate'])],
                          cwd=root, stdout=log, stderr=subprocess.STDOUT)
metadata['measurement_exit'] = proc.returncode
metadata['source_unchanged'] = True
metadata['status'] = 'finished' if proc.returncode == 0 else 'failed'
verify()
assert all(sha(bins[role]) == info['binary_sha256'] for role, info in metadata['builds'].items())
(out/'build-provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
assert proc.returncode == 0
decision = json.loads((out/'measurement/decision.json').read_text())
assert decision['complete'] and decision['lifecycle_resource_gate_passes'], 'Frozen lifecycle gate failed; preserve all windows'
