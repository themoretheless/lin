from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys

assets = Path(__file__).resolve().parent
root = assets.parents[2]
out = Path('/tmp/lin-embedding-caller-linux')
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
bins = {}
for role in ['candidate', 'baseline']:
    ref = metadata['head'] if role == 'candidate' else spec['baseline_reference']
    path = root if role == 'candidate' else Path('/tmp/lin-embedding-caller-baseline-source')
    if role == 'baseline':
        subprocess.run(['git', 'worktree', 'add', '--detach', str(path), ref], check=True)
    source = closure(path)
    if role == 'baseline':
        candidate = metadata['builds']['candidate']['source_sha256']
        assert [name for name in source if source[name] != candidate[name]] == ['src/embed.rs']
    env = dict(os.environ, CARGO_TARGET_DIR=str(root/'target'), CARGO_BUILD_JOBS='2')
    with (out/f'build-{role}.log').open('w') as log:
        subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'lin-peer-worker'], cwd=path,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
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
print(json.dumps(decision), flush=True)
assert decision['complete'] and decision['component_gate_passes'], 'Numeric gate failed; retain every loss'
