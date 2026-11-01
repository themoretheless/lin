"""Retain the entire corrected-order run, including a failed source gate."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile

p = Path(__file__).resolve().parent
repo = p.parents[2]
run = '41353987944'
expected = 'db1d4b7503ef2557f0af35fa1407793ab118fcdc'
meta = json.loads(subprocess.check_output(
    ['gh', 'run', 'view', run, '--json', 'databaseId,status,conclusion,headSha,url,jobs'], cwd=repo, text=True))
assert meta['status'] == 'completed' and meta['headSha'] == expected
assert meta['conclusion'] in ('success', 'failure')
steps = [s for j in meta['jobs'] for s in j['steps']]
assert [s['name'] for s in steps if s['conclusion'] == 'failure'] in ([], ['Enforce unchanged source acceptance gate'])
assert next(s for s in steps if s['name'] == 'Independent raw verification')['conclusion'] == 'success'
raw = Path('/tmp/lin-corrected-cow-balanced-'+run)
assert not raw.exists(), 'Preserve previous downloads'
raw.mkdir()
subprocess.run(['gh', 'run', 'download', run, '--name', 'corrected-cow-full-engine', '--dir', str(raw)], cwd=repo, check=True)
q = json.loads((raw/'qualification.json').read_text())
assert q['measurement_complete'] and q['processes'] == 2412 and q['observations'] == 19296
assert not q['full_objective_achieved']
frozen = json.loads((raw/'frozen-runner.json').read_text())
for name, digest in frozen.items():
    data = subprocess.check_output(['git', 'show', expected+':'+name], cwd=repo)
    assert hashlib.sha256(data).hexdigest() == digest, name
with tempfile.TemporaryDirectory(prefix='lin-cow-balanced-recompute-') as td:
    copied = Path(td)/'raw'
    shutil.copytree(raw, copied)
    subprocess.run(['python3', str(copied/'summarize.py')], cwd=repo, check=True)
    for name in ['summary.json', 'qualification.json', 'peer-comparisons.json']:
        assert json.loads((raw/name).read_text()) == json.loads((copied/name).read_text()), name
out = repo/'benches/results/2026-11-02-corrected-cow-balanced-linux'
out.mkdir(exist_ok=False)
files = sorted(f for f in raw.rglob('*') if f.is_file())
manifest = {str(f.relative_to(raw)): dict(bytes=f.stat().st_size, sha256=hashlib.sha256(f.read_bytes()).hexdigest()) for f in files}
archive = out/'raw.tar.gz'
with tarfile.open(archive, 'w:gz') as tf:
    for f in files:
        tf.add(f, arcname=str(f.relative_to(raw)), recursive=False)
with tarfile.open(archive) as tf:
    assert len(tf.getmembers()) == len(manifest)
    for member in tf:
        data = tf.extractfile(member).read()
        assert len(data) == manifest[member.name]['bytes']
        assert hashlib.sha256(data).hexdigest() == manifest[member.name]['sha256']
for name in ['summary.json', 'qualification.json', 'peer-comparisons.json', 'report.md']:
    shutil.copy2(raw/name, out/name)
for name, value in [('run-metadata.json', meta), ('raw-manifest.json', manifest), ('archive-audit.json', dict(
    files=len(files), all_member_hashes_verified=True, numerical_recompute_identical=True,
    archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(), binary_payloads_archived=False,
    ci_independent_raw_verification_passed=True, prior_failed_run=41306613457,
    runtime_adopted=False, full_objective_achieved=False))]:
    (out/name).write_text(json.dumps(value, indent=2)+'\n')
print(json.dumps(dict(archive=str(out), files=len(files), source_gate=q['source_candidate_qualified'])))
