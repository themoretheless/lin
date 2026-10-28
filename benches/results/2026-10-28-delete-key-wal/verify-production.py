from pathlib import Path
import subprocess,json,hashlib,tarfile
p=Path(__file__).resolve().parent;prior=p.parent/'2026-10-28-delete-key-replay-probe';example=Path('examples/delete_key_production_fixtures.rs');assert not example.exists();hashes={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=hashes,performance_qualified=False)
try:
 manifest=json.loads((prior/'fixture-manifest.json').read_text())['files_sha256'];assert all(hashlib.sha256((prior/'fixtures'/f).read_bytes()).hexdigest()==h for f,h in manifest.items())
 example.write_bytes((p/'production-fixtures.rs').read_bytes());m['probe_sha256']=hashlib.sha256(example.read_bytes()).hexdigest()
 with(p/'production-build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','delete_key_production_fixtures','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 m['build_exit']=r.returncode;assert r.returncode==0
 def binary(log,name):
  found=[]
  for line in log.read_text().splitlines():
   try:q=json.loads(line)
   except ValueError:continue
   if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')==name and q.get('executable'):found.append(q['executable'])
  assert len(found)==1;return found[0]
 producer=binary(p/'production-build.log','delete_key_production_fixtures');m['producer_binary_sha256']=hashlib.sha256(Path(producer).read_bytes()).hexdigest()
 with(p/'production-outcomes.jsonl').open('w')as out,(p/'production-runtime.log').open('w')as log:r=subprocess.run([producer,str(prior/'fixtures'),str(p/'production-fixtures')],stdout=out,stderr=log)
 m['producer_exit']=r.returncode;assert r.returncode==0
 legacy=binary(prior/'legacy-build.log','delete_key_legacy_reader');old=json.loads((prior/'legacy-provenance.json').read_text());assert hashlib.sha256(Path(legacy).read_bytes()).hexdigest()==old['binary_sha256'];m['legacy_reference']=old['legacy_reference'];m['legacy_binary_sha256']=old['binary_sha256']
 with(p/'production-legacy-outcomes.jsonl').open('w')as out,(p/'production-legacy-runtime.log').open('w')as log:r=subprocess.run([legacy,str(p/'production-fixtures')],stdout=out,stderr=log)
 m['legacy_exit']=r.returncode;assert r.returncode==0
 for name,field in [('production-outcomes.jsonl','production_bytes_exactly_match_legacy_qualified_fixture'),('production-legacy-outcomes.jsonl','legacy_primary_reader_passed')]:
  rows=[json.loads(s)for s in(p/name).read_text().splitlines()];assert len(rows)==8 and all(r[field]for r in rows)
 m['production_legacy_cases_verified']=8
finally:
 example.unlink(missing_ok=True);m['all_production_sources_restored']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in hashes.items());(p/'production-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['all_production_sources_restored']
print('all eight production Full streams exactly match golden bytes and pass frozen legacy Db replay')
