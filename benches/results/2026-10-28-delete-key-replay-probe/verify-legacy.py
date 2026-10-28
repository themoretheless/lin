from pathlib import Path
import subprocess,json,hashlib,tempfile,os
ROOT=Path.cwd();p=Path(__file__).resolve().parent;ref='04749994b9ca7a090476b95401f0f7166d8b870a';snapshot=Path(tempfile.mkdtemp(prefix='lin-delete-key-legacy-'));m=dict(legacy_reference=ref,snapshot=str(snapshot),source_sha256={},performance_qualified=False,current_source_changed=False)
current={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
try:
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','lin-derive','tools/peer-worker','Cargo.toml','Cargo.lock','benches/compare.rs'],text=True).splitlines()
 for name in names:
  data=subprocess.check_output(['git','show',ref+':'+name]);f=snapshot/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(data);m['source_sha256'][name]=hashlib.sha256(data).hexdigest()
 example=snapshot/'examples/delete_key_legacy_reader.rs';example.parent.mkdir();example.write_bytes((p/'legacy-reader.rs').read_bytes());m['probe_sha256']=hashlib.sha256(example.read_bytes()).hexdigest()
 with(p/'legacy-build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--locked','--example','delete_key_legacy_reader','--message-format=json-render-diagnostics'],cwd=snapshot,stdout=log,stderr=subprocess.STDOUT)
 m['build_exit']=r.returncode;assert r.returncode==0
 found=[]
 for line in(p/'legacy-build.log').read_text().splitlines():
  try:q=json.loads(line)
  except ValueError:continue
  if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='delete_key_legacy_reader'and q.get('executable'):found.append(q['executable'])
 assert len(found)==1;m['binary_sha256']=hashlib.sha256(Path(found[0]).read_bytes()).hexdigest()
 with(p/'legacy-outcomes.jsonl').open('w')as out,(p/'legacy-runtime.log').open('w')as log:r=subprocess.run([found[0],str(p/'fixtures')],stdout=out,stderr=log)
 m['runtime_exit']=r.returncode;assert r.returncode==0
 rows=[json.loads(s)for s in(p/'legacy-outcomes.jsonl').read_text().splitlines()];assert len(rows)==8 and all(q['legacy_primary_reader_passed']for q in rows);m['legacy_cases_verified']=8
finally:
 m['current_sources_unchanged']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in current.items());m['legacy_sources_unchanged']=all(hashlib.sha256((snapshot/f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items());(p/'legacy-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['current_sources_unchanged']and m['legacy_sources_unchanged']
print('eight bounded legacy-reader fixtures passed; no performance qualification')
