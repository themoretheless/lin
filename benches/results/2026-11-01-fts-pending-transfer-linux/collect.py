from pathlib import Path
import hashlib,json,subprocess,tarfile,tempfile,shutil
repo=Path('/Users/themoretheless/.codex/worktrees/lin-fts-pending-transfer/lin')
run='41336168475'; expected='ee14069caca55f54c2cbca9e5c32eebadabf350d'
def gh(*args): return subprocess.check_output(['gh',*args],cwd=repo,text=True)
meta=json.loads(gh('run','view',run,'--json','status,conclusion,headSha,url,jobs'))
assert meta['status']=='completed', 'Original run is still live; do not restart it'
assert meta['headSha']==expected
raw=Path('/tmp/lin-fts-pending-transfer-'+run)
assert not raw.exists(), 'Keep previous download; inspect before reuse'
raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','fts-pending-transfer','--dir',str(raw)],cwd=repo,check=True)
q=json.loads((raw/'measurement/qualification.json').read_text())
assert q['measurement_complete'] and q['processes']==756 and q['observations']==6048
assert q['full_objective_achieved'] is False
# Recompute complete numerical results from downloaded raw observations.
with tempfile.TemporaryDirectory(prefix='lin-transfer-recompute-') as t:
    copy=Path(t)/'raw';shutil.copytree(raw,copy)
    subprocess.run(['python3',str(copy/'analyze.py'),str(copy)],check=True)
    for name in ['summary.json','qualification.json']:
        assert json.loads((raw/'measurement'/name).read_text())==json.loads((copy/'measurement'/name).read_text()),name
out=repo/'benches/results/2026-11-01-fts-pending-transfer-linux'
out.mkdir(exist_ok=False)
files=sorted(f for f in raw.rglob('*') if f.is_file())
manifest={str(f.relative_to(raw)):{'sha256':hashlib.sha256(f.read_bytes()).hexdigest(),'bytes':f.stat().st_size}for f in files}
archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tf:
    for f in files:tf.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tf:
    members=tf.getmembers();assert len(members)==len(manifest)
    for m in members:
        assert m.isfile()
        b=tf.extractfile(m).read();assert len(b)==manifest[m.name]['bytes'] and hashlib.sha256(b).hexdigest()==manifest[m.name]['sha256']
for n in ['summary.json','qualification.json','report.md']:
    shutil.copy2(raw/'measurement'/n,out/n)
for name,data in [('run-meta.json',meta),('raw-manifest.json',manifest),('archive-audit.json',dict(all_member_hashes_verified=True,files=len(files),numerical_recompute_identical=True,archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),binary_payloads_archived=False,full_objective_achieved=False))]:
    (out/name).write_text(json.dumps(data,indent=2)+'\n')
print(json.dumps({'archive':str(out),'files':len(files),'qualification':q}))
