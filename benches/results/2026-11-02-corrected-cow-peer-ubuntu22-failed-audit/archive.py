from pathlib import Path
import hashlib,json,tarfile
p=Path(__file__).resolve().parent
source=Path('/tmp/lin-cow-peer-ubuntu22-41376860938')
assert source.is_dir() and not(p/'raw.tar.gz').exists(), 'preserve previous archive'
sha=lambda b:hashlib.sha256(b).hexdigest()
files=sorted(f for f in source.rglob('*')if f.is_file())
assert not any(f.is_symlink()for f in source.rglob('*'))
manifest={str(f.relative_to(source)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
records=[]
for f in sorted(source.rglob('run.json')):
 d=json.loads(f.read_text());before=set(d.get('git_status_before','').splitlines());after=set(d.get('git_status_after','').splitlines())
 added=sorted(after-before);removed=sorted(before-after)
 prefix='?? benches/results/2026-11-02-corrected-cow-peer-ubuntu22/'
 generated=not removed and all(line.startswith(prefix)and line.endswith('/')for line in added)
 assert generated,(str(f),added,removed)
 assert d['source_drift']==bool(added or removed)
 records.append(dict(path=str(f.relative_to(source)),original_source_drift=d['source_drift'],added_status=added,removed_status=removed,status_delta_is_generated_output_only=generated))
assert len(records)==24
with tarfile.open(p/'raw.tar.gz','w:gz')as t:
 for f in files:t.add(f,arcname=str(f.relative_to(source)),recursive=False)
with tarfile.open(p/'raw.tar.gz','r:gz')as t:
 assert sorted(t.getnames())==sorted(manifest)
 for member in t.getmembers():assert sha(t.extractfile(member).read())==manifest[member.name]['sha256']
(p/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(p/'classification.json').write_text(json.dumps(dict(run_id=41376860938,job_conclusion='failure',failure_step='Audit all observations values and original processes',original_flags_preserved=True,archive_files=len(files),archive_sha256=sha((p/'raw.tar.gz').read_bytes()),report_count=len(records),drift_reports=sum(r['original_source_drift']for r in records),qualification_passed=False,scope='Classification of reported Git-status deltas only; not proof of complete source-content integrity or numerical acceptance',reports=records),indent=2)+'\n')
print(json.dumps(dict(archived_files=len(files),reports=len(records),drift_reports=sum(r['original_source_drift']for r in records),qualification_passed=False)))
