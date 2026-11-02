from pathlib import Path
import hashlib,json,tarfile
p=Path(__file__).resolve().parent
q=json.loads((p/'independent-audit.json').read_text());assert q['audit_complete']and q['processes']==216 and q['observations']==1728
archive=p/'raw.tar.gz';assert not archive.exists(),'retain earlier archive'
files=sorted(f for f in p.rglob('*')if f.is_file()and '__pycache__'not in f.parts)
manifest={str(f.relative_to(p)):dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())for f in files}
with tarfile.open(archive,'w:gz')as tar:
 for f in files:tar.add(f,arcname=str(f.relative_to(p)),recursive=False)
with tarfile.open(archive)as tar:
 assert len(tar.getmembers())==len(manifest)
 for member in tar:
  data=tar.extractfile(member).read();assert len(data)==manifest[member.name]['bytes']and hashlib.sha256(data).hexdigest()==manifest[member.name]['sha256']
(p/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
r=dict(files=len(files),all_member_hashes_verified=True,archive_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),binary_payloads_in_git=False,performance_qualification=False,adopted=False)
(p/'archive-audit.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
