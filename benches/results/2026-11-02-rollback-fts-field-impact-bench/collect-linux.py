"""Retain the complete FTS component matrix, failed gates and exact Linux executables."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile

p = Path(__file__).resolve().parent
repo = p.parents[2]
run = '41366596325'
expected = '47d88985bc8f2010cc6272835d276ba6ed655056'
meta = json.loads(subprocess.check_output(['gh', 'run', 'view', run, '--json',
    'databaseId,status,conclusion,headSha,url,jobs'], cwd=repo, text=True))
assert meta['status'] == 'completed' and meta['headSha'] == expected
assert meta['conclusion'] in ('success', 'failure')
steps = [s for j in meta['jobs'] for s in j['steps']]
assert [s['name']for s in steps if s['conclusion']=='failure'] in ([],['Enforce unchanged component criteria'])
assert next(s for s in steps if s['name'] == 'Independent raw source command and numerical audit')['conclusion'] == 'success'
raw = Path('/tmp/lin-rollback-fts-field-impact-'+run)
assert not raw.exists(), 'Preserve earlier download'
raw.mkdir()
subprocess.run(['gh', 'run', 'download', run, '--name', 'rollback-fts-field-impact', '--dir', str(raw)], cwd=repo, check=True)
source = raw/p.name
frozen = json.loads((source/'frozen.json').read_text())
for name, digest in frozen.items():
    assert hashlib.sha256(subprocess.check_output(['git', 'show', expected+':'+name], cwd=repo)).hexdigest() == digest
    if name.startswith('benches/results/'):
        relative = Path(name).relative_to('benches/results')
        assert hashlib.sha256((raw/relative).read_bytes()).hexdigest() == digest, name
assert hashlib.sha256((repo/'scripts/benchmark_role_balance.py').read_bytes()).hexdigest() == frozen['scripts/benchmark_role_balance.py']
q = json.loads((source/'qualification.json').read_text())
audit = json.loads((source/'independent-audit.json').read_text())
assert q['measurement_complete'] and q['processes'] == 720 and q['observations'] == 5760
assert audit['audit_complete'] and not audit['synthetic_fixture']
with tempfile.TemporaryDirectory(prefix='lin-fts-impact-recompute-') as td:
    copied = Path(td)/p.name
    shutil.copytree(source, copied)
    subprocess.run(['python3', str(copied/'analyze.py'), str(copied)], cwd=repo, check=True)
    for name in ['summary.json', 'qualification.json']:
        assert json.loads((source/name).read_text()) == json.loads((copied/name).read_text())
binaries = Path('/tmp/lin-rollback-fts-field-impact-executables-'+run)
assert not binaries.exists(), 'Preserve earlier binary download'
binaries.mkdir()
subprocess.run(['gh', 'run', 'download', run, '--name', 'rollback-fts-field-impact-exact-linux-executables', '--dir', str(binaries)], cwd=repo, check=True)
provenance = json.loads((source/'provenance.json').read_text())
binary_records = {}
for role in ['baseline', 'candidate']:
    info = provenance['binaries'][role]
    binary = binaries/Path(info['path']).name
    data = binary.read_bytes()
    assert hashlib.sha256(data).hexdigest() == info['sha256']
    assert data[:6] == b'\x7fELF\x02\x01' and int.from_bytes(data[18:20], 'little') == 62
    binary_records[role] = dict(path=str(binary), sha256=info['sha256'], bytes=len(data), elf64_x86_64=True)
out = repo/'benches/results/2026-11-02-rollback-fts-field-impact-linux'
out.mkdir(exist_ok=False)
files = sorted(f for f in raw.rglob('*') if f.is_file())
manifest = {str(f.relative_to(raw)): dict(bytes=f.stat().st_size, sha256=hashlib.sha256(f.read_bytes()).hexdigest()) for f in files}
archive = out/'raw.tar.gz'
with tarfile.open(archive, 'w:gz') as tf:
    for f in files: tf.add(f, arcname=str(f.relative_to(raw)), recursive=False)
with tarfile.open(archive) as tf:
    assert len(tf.getmembers()) == len(manifest)
    for member in tf:
        data = tf.extractfile(member).read()
        assert len(data) == manifest[member.name]['bytes'] and hashlib.sha256(data).hexdigest() == manifest[member.name]['sha256']
for name in ['summary.json', 'qualification.json', 'report.md', 'independent-audit.json']:
    shutil.copy2(source/name, out/name)
subprocess.run(['python3', str(p/'compare-sqlite.py'), str(out)], cwd=repo, check=True)
summary = json.loads((out/'summary.json').read_text())
failed = [dict(case=case, series=series, **result) for case, rows in summary.items()
          for series, result in rows.items() if not result['passes']]
(out/'failed-windows.json').write_text(json.dumps(failed, indent=2)+'\n')
(out/'decision.json').write_text(json.dumps(dict(component_gate_passes=q['component_gate_passes'],
    baseline_commit='625bc7180', candidate_commit='65bc82590', adopted=False,
    full_engine_qualified=False, unchanged_retry=False, full_objective_achieved=False), indent=2)+'\n')
for name, value in [('run-metadata.json', meta), ('raw-manifest.json', manifest),
                    ('binary-artifact.json', dict(run_id=int(run), artifact='rollback-fts-field-impact-exact-linux-executables', verified_local_files=binary_records, binary_payloads_in_git=False)),
                    ('archive-audit.json', dict(files=len(files), all_member_hashes_verified=True,
                        numerical_recompute_identical=True, exact_executable_hashes_verified=True,
                        archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
                        runtime_adopted=False, full_objective_achieved=False))]:
    (out/name).write_text(json.dumps(value, indent=2)+'\n')
print(json.dumps(dict(archive=str(out), files=len(files), exact_binaries_verified=2, component_gate_passes=q['component_gate_passes'])))
