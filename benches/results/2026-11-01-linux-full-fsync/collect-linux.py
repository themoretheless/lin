from pathlib import Path
import subprocess,json,hashlib,tarfile,tempfile,shutil
repo=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;run='41341179993';expected='950854e4a2f9379fef2fad0398ff0bb37ed34159'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url,jobs'],cwd=repo,text=True));assert meta['status']=='completed'and meta['headSha']==expected
raw=Path('/tmp/lin-linux-full-fsync-'+run);assert not raw.exists();raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','linux-full-fsync','--dir',str(raw)],cwd=repo,check=True)
q=json.loads((raw/'measurement/qualification.json').read_text());assert q['measurement_complete']and q['processes']==720 and q['observations']==5760 and not q['full_objective_achieved']
out=repo/'benches/results/2026-11-01-linux-full-fsync-linux';out.mkdir(exist_ok=False)
with tempfile.TemporaryDirectory(prefix='lin-fsync-recompute-')as td:
 copied=Path(td)/'raw';shutil.copytree(raw,copied)
 subprocess.run(['python3',str(copied/'analyze.py'),str(copied)],check=True)
 for n in ['summary.json','qualification.json']:assert json.loads((raw/'measurement'/n).read_text())==json.loads((copied/'measurement'/n).read_text())
 subprocess.run(['python3',str(p/'compare-sqlite.py'),str(copied)],check=True)
 for n in ['sqlite-peer-ratios.json','sqlite-peer-ratios.md']:shutil.copy2(copied/'measurement'/n,out/n)
files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())for f in files};archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tf:
 for f in files:tf.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tf:
 assert len(tf.getmembers())==len(manifest)
 for m in tf.getmembers():
  b=tf.extractfile(m).read();assert len(b)==manifest[m.name]['bytes']and hashlib.sha256(b).hexdigest()==manifest[m.name]['sha256']
for n in ['summary.json','qualification.json','report.md']:shutil.copy2(raw/'measurement'/n,out/n)
shutil.copy2(raw/'independent-audit.json',out/'independent-audit.json')
s=json.loads((out/'summary.json').read_text());failed=[dict(case=c,series=n,**r)for c,v in s.items()for n,r in v.items()if not r['passes']]
for name,value in [('run-metadata.json',meta),('raw-manifest.json',manifest),('failed-windows.json',failed),('archive-audit.json',dict(files=len(files),all_member_hashes_verified=True,numerical_recompute_identical=True,archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),binary_payloads_archived=False,full_objective_achieved=False)),('decision.json',dict(component_gate_passes=q['component_gate_passes'],adopted=False,unchanged_retry=False,full_engine_qualified=False,full_objective_achieved=False))]:
 (out/name).write_text(json.dumps(value,indent=2)+'\n')
print(json.dumps({'archive':str(out),'files':len(files),'component_gate_passes':q['component_gate_passes'],'failed_windows':len(failed)}))
