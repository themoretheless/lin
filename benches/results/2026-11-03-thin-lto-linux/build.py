from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys, platform, re, time

assets = Path(__file__).resolve().parent
root = assets.parents[2]
out = Path('/tmp/lin-thin-lto-linux')
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
    path = root if role == 'candidate' else Path('/tmp/lin-thin-lto-baseline-source')
    if role == 'baseline':
        subprocess.run(['git', 'worktree', 'add', '--detach', str(path), ref], check=True)
    source = closure(path)
    if role == 'baseline':
        candidate = metadata['builds']['candidate']['source_sha256']
        assert [name for name in source if source[name] != candidate[name]] == ['Cargo.toml']
    env = dict(os.environ, CARGO_TARGET_DIR=str(root/'target'), CARGO_BUILD_JOBS='2')
    build_started = time.perf_counter()
    with (out/f'clean-{role}.log').open('w') as log:
        subprocess.run(['cargo', 'clean', '--release', '-p', 'lin', '-p', 'lin-peer-worker'], cwd=path, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    with (out/f'build-{role}.log').open('w') as log:
        subprocess.run(['cargo', 'build', '--locked', '--release', '--verbose', '-p', 'lin-peer-worker'], cwd=path,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    build_log = re.sub(r'\x1b\[[0-9;]*m', '', (out/f'build-{role}.log').read_text())
    assert re.search(r'Compiling lin v[0-9]', build_log) and re.search(r'Compiling lin-peer-worker v[0-9]', build_log)
    binary = out/f'{role}-worker'
    shutil.copy2(root/'target/release/lin-peer-worker', binary)
    assert source == closure(path)
    assert binary.read_bytes()[:6] == b'\x7fELF\x02\x01'
    bins[role] = binary
    metadata['builds'][role] = {'reference': ref, 'source_sha256': source, 'binary_sha256': sha(binary), 'binary_bytes': binary.stat().st_size, 'clean_and_build_wall_s': time.perf_counter()-build_started}
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
warm = json.loads((out/'measurement/decision.json').read_text())
metadata['status'] = 'measuring-resources'
(out/'build-provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
with (out/'resource.log').open('w') as log:
    resource = subprocess.run([sys.executable, str(root/'scripts/bench-native-lifecycle.py'), '--output', str(out/'resource'),
        '--baseline-binary', str(bins['baseline']), '--candidate-binary', str(bins['candidate'])],
        cwd=root, stdout=log, stderr=subprocess.STDOUT)
metadata['resource_exit'] = resource.returncode
metadata['status'] = 'finished' if resource.returncode == 0 else 'failed'
verify()
(out/'build-provenance.json').write_text(json.dumps(metadata, indent=2)+'\n')
assert resource.returncode == 0
cold = json.loads((out/'resource/decision.json').read_text())
decision = {'complete': warm['complete'] and cold['complete'], 'processes': 216,
    'timed_operation_samples': 6156, 'cold_process_wall_samples': 108, 'peak_rss_observations': 108,
    'warm_gate_passes': warm['component_gate_passes'], 'resource_gate_passes': cold['resource_gate_passes'],
    'native_source_resource_gate_passes': warm['component_gate_passes'] and cold['resource_gate_passes'],
    'full_peer_qualified': False, 'full_objective_achieved': False}
(out/'combined-decision.json').write_text(json.dumps(decision, indent=2)+'\n')
print(json.dumps(decision), flush=True)
assert decision['complete'] and decision['native_source_resource_gate_passes'], 'Frozen numeric gate failed; retain every loss'
