from pathlib import Path
import subprocess,json,hashlib,tarfile
p=Path(__file__).resolve().parent;example=Path('examples/wal_positive_zero_component_fixtures.rs');assert not example.exists()
source={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
m=json.loads((p/'fixture-manifest.json').read_text());assert hashlib.sha256((p/'legacy-qualified-fixtures.tar.gz').read_bytes()).hexdigest()==m['archive_sha256']
dest=p/'qualified-golden-input';dest.mkdir(exist_ok=False)
with tarfile.open(p/'legacy-qualified-fixtures.tar.gz')as tf:
 members=tf.getmembers();assert len(members)==32 and {x.name for x in members}==set(m['files_sha256'])
 for member in members:
  assert member.isfile()and Path(member.name).name==member.name
  data=tf.extractfile(member).read();assert hashlib.sha256(data).hexdigest()==m['files_sha256'][member.name];(dest/member.name).write_bytes(data)
status=dict(legacy_reference=m['legacy_reference'],golden_members_verified=32,performance_qualified=False)
try:
 example.write_bytes((p/'fixture-writer.rs').read_bytes())
 with(p/'golden-build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','wal_positive_zero_component_fixtures','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 status['build_exit']=r.returncode;assert r.returncode==0
 found=[]
 for line in(p/'golden-build.log').read_text().splitlines():
  try:q=json.loads(line)
  except ValueError:continue
  if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='wal_positive_zero_component_fixtures'and q.get('executable'):found.append(q['executable'])
 assert len(found)==1
 with(p/'golden-outcomes.jsonl').open('w')as out,(p/'golden-runtime.log').open('w')as log:r=subprocess.run([found[0],str(p/'generated-fixtures')],stdout=out,stderr=log)
 status['runtime_exit']=r.returncode;assert r.returncode==0
 cases=[json.loads(s)for s in(p/'golden-outcomes.jsonl').read_text().splitlines()];assert len(cases)==8 and all(c['current_replay_verified']for c in cases)
 assert {x.name for x in(p/'generated-fixtures').iterdir()}==set(m['files_sha256'])
 assert all(hashlib.sha256((p/'generated-fixtures'/n).read_bytes()).hexdigest()==h for n,h in m['files_sha256'].items())
 status['production_qualified_cases_verified']=8
finally:
 example.unlink(missing_ok=True);status['source_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in source.items());(p/'golden-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['source_restored']
