from pathlib import Path
import hashlib,json,math,tarfile
from assignment import binary_for,validate
p=Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).hexdigest()
m=json.loads((p/'provenance.json').read_text());validate(m)
status=json.loads((p/'local-smoke-status.json').read_text());cases=json.loads((p/'cases.json').read_text())
assert status['correctness_smoke_only']and not status['performance_qualified']and len(status['runs'])==134
assert {(r['role'],r['case'])for r in status['runs']}=={(role,c)for role in ['baseline','candidate']for c in cases}
external=0
for r in status['runs']:
 assigned,info=binary_for(m,r['role'],r['case']);assert r['exit_code']==0 and r['binary_role']==assigned and r['binary_sha256']==info['sha256']
 assert sha(Path(info['path']).read_bytes())==info['sha256'];external+=assigned=='shared_peer'
 d=json.loads((p/'local-smoke'/r['role']/r['case'].replace('/','_')/'run.json').read_text())
 assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==1
 o=d['observations'][0];assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0
assert external==76 and m['source_restored']
assert {str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')}==m['source_sha256']
assert sha(Path('benches/compare.rs').read_bytes())==m['original_compare_sha256']
assert sha((p/'compare.rs').read_bytes())==m['compiled_compare_sha256']
archive=p/'local-functional.tar.gz';assert not archive.exists(),'retain earlier archive'
files=sorted(f for directory in ['local-smoke','correctness-artifacts']for f in(p/directory).rglob('*')if f.is_file())
files+=[p/n for n in ['provenance.json','local-smoke-status.json','build-baseline.log','build-candidate.log','correctness-prerequisites.json']]
manifest={str(f.relative_to(p)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
with tarfile.open(archive,'w:gz')as tar:
 for f in files:tar.add(f,arcname=str(f.relative_to(p)),recursive=False)
with tarfile.open(archive)as tar:
 assert len(tar.getmembers())==len(manifest)
 for member in tar:
  b=tar.extractfile(member).read();assert len(b)==manifest[member.name]['bytes']and sha(b)==manifest[member.name]['sha256']
result=dict(files=len(files),all_member_hashes_verified=True,archive_sha256=sha(archive.read_bytes()),actual_smokes=134,invariant_external_smokes=76,source_restored=True,measured_processes=0,performance_qualification=False,binary_payloads_in_git=False)
(p/'local-functional-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(p/'local-qualification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
