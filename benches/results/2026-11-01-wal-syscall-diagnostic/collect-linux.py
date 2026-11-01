from pathlib import Path
import subprocess,json,hashlib,tarfile,tempfile,shutil
repo=Path(__file__).resolve().parents[3]
p=repo/'benches/results/2026-11-01-wal-syscall-diagnostic';run='41338904938';expected='1bafaf7c388ddc90808db103cb59b37cfa49f9f4'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url,jobs'],cwd=repo,text=True))
assert meta['status']=='completed' and meta['headSha']==expected
raw=Path('/tmp/lin-wal-syscall-diagnostic-'+run);assert not raw.exists();raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','wal-syscall-diagnostic','--dir',str(raw)],cwd=repo,check=True)
assert meta['conclusion']=='success','Retain failed original artifact before deciding next action'
out=repo/'benches/results/2026-11-01-wal-syscall-diagnostic-linux';out.mkdir(exist_ok=False)
with tempfile.TemporaryDirectory(prefix='lin-syscall-audit-')as t:
 copied=Path(t)/'raw';shutil.copytree(raw,copied)
 subprocess.run(['python3',str(p/'audit-traces.py'),str(copied)],check=True)
 for n in ['independent-trace-audit.json','syscall-report.md']:shutil.copy2(copied/n,out/n)
files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())for f in files}
archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tf:
 for f in files:tf.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tf:
 assert len(tf.getmembers())==len(manifest)
 for m in tf.getmembers():
  b=tf.extractfile(m).read();assert len(b)==manifest[m.name]['bytes'] and hashlib.sha256(b).hexdigest()==manifest[m.name]['sha256']
for name,data in [('run-metadata.json',meta),('raw-manifest.json',manifest),('archive-audit.json',dict(files=len(files),all_member_hashes_verified=True,independent_trace_reconstruction=True,binary_payload_rehashed=False,performance_qualification=False,full_objective_achieved=False,archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest()))]:
 (out/name).write_text(json.dumps(data,indent=2)+'\n')
print(str(out))
