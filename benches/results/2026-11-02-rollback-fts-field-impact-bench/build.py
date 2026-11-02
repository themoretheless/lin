from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
sys.path.insert(0, str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable

p = Path(__file__).resolve().parent
m = json.loads((p/'input.json').read_text())
sha = lambda data: hashlib.sha256(data).hexdigest()
functional = p.parent/'2026-11-02-rollback-fts-field-impact'
audit = json.loads((functional/'functional-audit.json').read_text())
assert audit['complete'] and audit['passed'] == 460 and audit['compiled_runtime_negatives'] == 2
before = {f: f.read_bytes() for f in Path('src').rglob('*.rs')}
assert {str(f): sha(b) for f, b in before.items()} == m['candidate']['source_sha256']
sources = {}
for role in ['baseline', 'candidate']:
    info = m[role]
    names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', info['commit'], 'src'], text=True).splitlines()
    assert {n for n in names if n.endswith('.rs')} == set(info['source_sha256'])
    sources[role] = {}
    for name, digest in info['source_sha256'].items():
        data = subprocess.check_output(['git', 'show', info['commit']+':'+name])
        assert sha(data) == digest
        sources[role][name] = data
    for name, digest in m['build_sha256'].items():
        assert sha(Path(name).read_bytes()) == digest
        assert sha(subprocess.check_output(['git', 'show', info['commit']+':'+name])) == digest
bench = Path('benches/compare.rs'); original_bench = bench.read_bytes()
meta = dict(source_sha256=m['candidate']['source_sha256'], original_compare_sha256=sha(original_bench),
            compiled_compare_sha256=sha((p/'compare.rs').read_bytes()), binaries={},
            instrumentation=False, source_restored=False)
env = dict(os.environ)
env.setdefault('CARGO_TARGET_DIR', 'target/rollback-fts-field-impact')
env.setdefault('CARGO_BUILD_JOBS', '2')
try:
    bench.write_bytes((p/'compare.rs').read_bytes())
    for role in ['baseline', 'candidate']:
        for f in Path('src').rglob('*.rs'):
            if str(f) not in sources[role]: f.unlink()
        for name, data in sources[role].items():
            Path(name).parent.mkdir(parents=True, exist_ok=True)
            Path(name).write_bytes(data)
        log = p/('build-'+role+'.log')
        with log.open('w') as output:
            result = subprocess.run(['cargo', 'bench', '--bench', 'compare', '--no-run', '--color', 'never'], env=env, stdout=output, stderr=subprocess.STDOUT)
        assert result.returncode == 0, role
        executable = Path(bench_executable(log.read_text(), 'compare')).resolve()
        target = Path(env['CARGO_TARGET_DIR']).resolve()
        assert executable.is_relative_to(target)
        dest = Path('/tmp/lin-rollback-fts-field-impact-'+role)
        shutil.copy2(executable, dest)
        meta['binaries'][role] = dict(path=str(dest), sha256=sha(dest.read_bytes()),
                                     source_sha256=m[role]['source_sha256'], cargo_target_dir=str(target))
finally:
    for f in Path('src').rglob('*.rs'):
        if f not in before: f.unlink()
    for f, data in before.items():
        f.parent.mkdir(parents=True, exist_ok=True); f.write_bytes(data)
    bench.write_bytes(original_bench)
    meta['source_restored'] = all(f.read_bytes() == b for f, b in before.items()) and bench.read_bytes() == original_bench
    (p/'provenance.json').write_text(json.dumps(meta, indent=2)+'\n')
assert meta['source_restored'] and len(meta['binaries']) == 2
assert meta['binaries']['baseline']['sha256'] != meta['binaries']['candidate']['sha256']
