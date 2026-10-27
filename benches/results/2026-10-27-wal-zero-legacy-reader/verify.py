from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent
read=lambda n:json.loads((p/n).read_text())
h=lambda b:hashlib.sha256(b).hexdigest()
m=read('provenance.json');s=read('status.json');r=read('legacy.json');c=read('controls.json')
assert m['source_restored'] and not m['performance_qualified']
for f,digest in m['source_sha256'].items():
 assert h(subprocess.check_output(['git','show',m['source_commit']+':'+f]))==digest,f
assert h((p/'probe.rs').read_bytes())==m['probe_sha256']
old=h(subprocess.check_output(['git','show','04749994b9ca7a090476b95401f0f7166d8b870a:src/persist.rs']))
assert old==m['binaries']['legacy']['persist_sha256']==m['source_sha256']['src/persist.rs']
assert h((p.parent/'2026-10-27-wal-zero-full-engine/candidate-persist.rs').read_bytes())==m['candidate_persist_sha256']==m['binaries']['producer']['persist_sha256']
assert m['binaries']['legacy']['sha256']!=m['binaries']['producer']['sha256']
for role in ['legacy','producer']:
 assert s[role]['exit_code']==0 and s[role]['binary_sha256']==m['binaries'][role]['sha256']
assert all(r[k] is True for k in ['legacy_reader_passed','full_canonical_rows','ieee_edge_record'])
assert [(x['phase'],x['generation'],x['applied']) for x in r['outcomes']]==[(1,1,1),(2,2,1),(3,3,1),(4,3,0),(5,4,1)]
for x in r['outcomes'][1:3]:assert 0<x['frame_bytes']<x['canonical_frame_bytes']
assert r['outcomes'][3]['frame_bytes']==0
assert r['outcomes'][4]['frame_bytes']==r['outcomes'][4]['canonical_frame_bytes']
for name in ['frame_corruption','expected_rows_changed']:
 assert c[name]['exit_code']!=0 and c[name]['runtime_control_caught']
 assert 'panicked at' in (p/(name+'.log')).read_text()
manifest=read('manifest.json')
for name,entry in manifest.items():
 data=(p/name).read_bytes()
 assert len(data)==entry['bytes'] and h(data)==entry['sha256'],name
print('PASS: frozen source provenance, distinct binaries, complete rows/generations, compact and fallback frames, IEEE record and negative controls')
