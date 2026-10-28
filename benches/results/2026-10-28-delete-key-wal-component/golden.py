from pathlib import Path
import subprocess,json,hashlib,tarfile
p=Path(__file__).resolve().parent;example=Path('examples/delete_key_component_golden.rs');assert not example.exists();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=json.loads((p/'legacy-golden-manifest.json').read_text());assert hashlib.sha256((p/'legacy-golden.tar.gz').read_bytes()).hexdigest()==m['archive_sha256'];dest=p/'golden-input';dest.mkdir(exist_ok=False)
with tarfile.open(p/'legacy-golden.tar.gz','r:gz')as tf:
 members=tf.getmembers();assert len(members)==len(m['files_sha256'])
 for member in members:
  assert member.isfile()and member.name in m['files_sha256']and Path(member.name).name==member.name
  data=tf.extractfile(member).read();assert hashlib.sha256(data).hexdigest()==m['files_sha256'][member.name];(dest/member.name).write_bytes(data)
status=dict(golden_members_verified=len(members),performance_qualified=False)
try:
 example.write_bytes((p/'production-fixtures.rs').read_bytes())
 with(p/'golden-build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','delete_key_component_golden','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 found=[]
 for line in(p/'golden-build.log').read_text().splitlines():
  try:q=json.loads(line)
  except ValueError:continue
  if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='delete_key_component_golden'and q.get('executable'):found.append(q['executable'])
 assert len(found)==1
 with(p/'golden-outcomes.jsonl').open('w')as out,(p/'golden-runtime.log').open('w')as log:r=subprocess.run([found[0],str(dest),str(p/'golden-generated')],stdout=out,stderr=log)
 assert r.returncode==0
 rows=[json.loads(s)for s in(p/'golden-outcomes.jsonl').read_text().splitlines()];assert len(rows)==8 and all(r['production_bytes_exactly_match_legacy_qualified_fixture']for r in rows)
 assert all(hashlib.sha256((p/'golden-generated'/name).read_bytes()).hexdigest()==h for name,h in m['files_sha256'].items());status['production_cases_verified']=8
finally:
 example.unlink(missing_ok=True);status['all_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'golden-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['all_sources_restored']
