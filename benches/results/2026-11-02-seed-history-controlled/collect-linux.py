"""Collect the frozen balanced diagnostic; never overwrite the cancelled attempt."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile

p = Path(__file__).resolve().parent
repo = p.parents[2]
run = '41353571356'
expected = 'c73b1af46b4b9957d97a2b1dfb9958bc47546f65'
meta = json.loads(subprocess.check_output(
    ['gh', 'run', 'view', run, '--json', 'status,conclusion,headSha,url,jobs'], cwd=repo, text=True))
assert meta['status'] == 'completed' and meta['headSha'] == expected
assert meta['conclusion'] == 'success', 'Retain failed artifact separately; no complete diagnostic claim'
raw = Path('/tmp/lin-seed-history-controlled-balanced-'+run)
assert not raw.exists(), 'Preserve previous downloads'
raw.mkdir()
subprocess.run(['gh', 'run', 'download', run, '--name', 'seed-history-controlled', '--dir', str(raw)], cwd=repo, check=True)
source = raw/p.name
status = json.loads((source/'measurement/diagnostic-status.json').read_text())
audit = json.loads((source/'independent-audit.json').read_text())
assert status['complete'] and status['processes'] == 576 and status['observations'] == 4608
assert audit['complete'] and audit['processes'] == 576 and audit['observations'] == 4608
assert not status['full_objective_achieved'] and not status['performance_qualification']
with tempfile.TemporaryDirectory(prefix='lin-seed-history-recompute-') as td:
    copied = Path(td)/p.name
    shutil.copytree(source, copied)
    subprocess.run(['python3', str(copied/'analyze.py'), str(copied)], cwd=repo, check=True)
    for name in ['summary.json', 'diagnostic-status.json']:
        assert json.loads((source/'measurement'/name).read_text()) == json.loads((copied/'measurement'/name).read_text())
out = repo/'benches/results/2026-11-02-seed-history-controlled-balanced-linux'
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
for name in ['summary.json', 'diagnostic-status.json', 'report.md']:
    shutil.copy2(source/'measurement'/name, out/name)
shutil.copy2(source/'independent-audit.json', out/'independent-audit.json')
for name, value in [('run-metadata.json', meta), ('raw-manifest.json', manifest), ('archive-audit.json', dict(
    files=len(files), all_member_hashes_verified=True, numerical_recompute_identical=True,
    archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(), binary_payloads_archived=False,
    prior_cancelled_run=41349896598, runtime_adopted=False, full_objective_achieved=False))]:
    (out/name).write_text(json.dumps(value, indent=2)+'\n')
print(json.dumps(dict(archive=str(out), files=len(files), performance_qualification=False)))
