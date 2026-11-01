from pathlib import Path
import subprocess,json,hashlib,tarfile,tempfile,shutil
repo=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;run='41353571509';expected='c73b1af46b4b9957d97a2b1dfb9958bc47546f65'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url,jobs'],cwd=repo,text=True));assert meta['status']=='completed'and meta['headSha']==expected
assert meta['conclusion'] in ('success','failure')
failed_steps=[s['name'] for j in meta['jobs'] for s in j['steps'] if s['conclusion']=='failure']
assert failed_steps in ([],['Enforce unchanged component criteria']), failed_steps
raw=Path('/tmp/lin-columnar-batch-wal-'+run);assert not raw.exists();raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','columnar-batch-wal','--dir',str(raw)],cwd=repo,check=True)
q=json.loads((raw/'benchmark-measurement/qualification.json').read_text());assert q['measurement_complete']and q['processes']==864 and q['observations']==6912 and not q['full_objective_achieved']
import sys
sys.path.insert(0,str(repo/'scripts'))
from benchmark_role_balance import orders_from_records
records=json.loads((raw/'benchmark-measurement/process-status.json').read_text())
cases=json.loads((raw/'benchmark-cases.json').read_text())
orders_from_records(records,[c['id'] for c in cases],['baseline','control','candidate'],'id')
assert (raw/'benchmark-independent-audit.json').is_file()
out=repo/'benches/results/2026-11-02-columnar-batch-wal-balanced-linux';out.mkdir(exist_ok=False)
with tempfile.TemporaryDirectory(prefix='lin-column-batch-recompute-')as td:
 copied=Path(td)/'raw';shutil.copytree(raw,copied)
 subprocess.run(['python3',str(copied/'analyze.py'),str(copied)],check=True)
 for n in ['summary.json','qualification.json']:assert json.loads((raw/'benchmark-measurement'/n).read_text())==json.loads((copied/'benchmark-measurement'/n).read_text())
 subprocess.run(['python3',str(p/'compare-sqlite.py'),str(copied)],check=True)
 for n in ['sqlite-peer-ratios.json','sqlite-peer-ratios.md']:shutil.copy2(copied/'benchmark-measurement'/n,out/n)
files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())for f in files};archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tf:
 for f in files:tf.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tf:
 assert len(tf.getmembers())==len(manifest)
 for m in tf.getmembers():
  b=tf.extractfile(m).read();assert len(b)==manifest[m.name]['bytes']and hashlib.sha256(b).hexdigest()==manifest[m.name]['sha256']
for n in ['summary.json','qualification.json','report.md']:shutil.copy2(raw/'benchmark-measurement'/n,out/n)
shutil.copy2(raw/'benchmark-independent-audit.json',out/'independent-audit.json')
s=json.loads((out/'summary.json').read_text());failed=[dict(case=c,series=n,**r)for c,v in s.items()for n,r in v.items()if not r['passes']]
for name,value in [('run-metadata.json',meta),('raw-manifest.json',manifest),('failed-windows.json',failed),('archive-audit.json',dict(files=len(files),all_member_hashes_verified=True,numerical_recompute_identical=True,archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),binary_payloads_archived=False,full_objective_achieved=False)),('decision.json',dict(component_gate_passes=q['component_gate_passes'],prior_failed_run=41344716657,corrected_role_order=True,adopted=False,unchanged_retry=False,full_engine_qualified=False,full_objective_achieved=False))]:
 (out/name).write_text(json.dumps(value,indent=2)+'\n')
print(json.dumps({'archive':str(out),'files':len(files),'component_gate_passes':q['component_gate_passes'],'failed_windows':len(failed)}))
