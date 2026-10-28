from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'legacy-provenance.json').read_text());assert m['legacy_cases_verified']==8
binaries=[]
for line in(p/'legacy-build.log').read_text().splitlines():
 try:q=json.loads(line)
 except ValueError:continue
 if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')=='delete_key_legacy_reader'and q.get('executable'):binaries.append(q['executable'])
assert len(binaries)==1 and hashlib.sha256(Path(binaries[0]).read_bytes()).hexdigest()==m['binary_sha256']
manifest=json.loads((p/'fixture-manifest.json').read_text())['files_sha256'];files={name:(p/'fixtures'/name).read_bytes()for name in manifest};assert all(hashlib.sha256(data).hexdigest()==manifest[name]for name,data in files.items());status=[]
try:
 for name,label in [('id_uri.expected','expected_state_changed'),('id_uri-keys.wal','key_frame_corrupted')]:
  f=p/'fixtures'/name;data=bytearray(files[name]);data[-1]^=1;f.write_bytes(data)
  with(p/f'legacy-negative-{label}.log').open('w')as log:r=subprocess.run([binaries[0],str(p/'fixtures')],stdout=log,stderr=subprocess.STDOUT)
  text=(p/f'legacy-negative-{label}.log').read_text();assert r.returncode!=0 and 'panicked'in text;status.append(dict(control=label,exit=r.returncode,runtime_rejected=True));f.write_bytes(files[name])
finally:
 for name,data in files.items():(p/'fixtures'/name).write_bytes(data)
 restored=all(hashlib.sha256((p/'fixtures'/name).read_bytes()).hexdigest()==h for name,h in manifest.items());(p/'legacy-negative-status.json').write_text(json.dumps(dict(controls=status,all_fixture_bytes_restored=restored),indent=2)+'\n')
assert restored and len(status)==2
r=subprocess.run([binaries[0],str(p/'fixtures')],capture_output=True,text=True);assert r.returncode==0
print('two legacy runtime corruptions rejected; every fixture restored; all eight positive cases pass again')
