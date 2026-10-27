from pathlib import Path
import json,hashlib,subprocess,shutil
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');before=source.read_bytes();example=Path('examples/wal_zero_compat_probe.rs');assert not example.exists();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();candidate=Path('benches/results/2026-10-28-wal-direct-batch/candidate-persist.rs')
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},probe_sha256=h(p/'probe.rs'),candidate_persist_sha256=h(candidate),binaries={},performance_qualified=False)
assert hashlib.sha256(subprocess.check_output(['git','show','04749994b9ca7a090476b95401f0f7166d8b870a:src/persist.rs'])).hexdigest()==h(source)
try:
 example.write_bytes((p/'probe.rs').read_bytes())
 for role in ['legacy','producer']:
  source.write_bytes(before if role=='legacy'else candidate.read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','wal_zero_compat_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  artifacts=[]
  for line in(p/f'build-{role}.log').read_text().splitlines():
   try:record=json.loads(line)
   except ValueError:continue
   if record.get('reason')=='compiler-artifact'and record.get('target',{}).get('name')=='wal_zero_compat_probe'and record.get('executable'):artifacts.append(record['executable'])
  assert len(artifacts)==1;dest=Path('/tmp/lin-wal-direct-batch-compat-'+role);shutil.copy2(artifacts[0],dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest),persist_sha256=h(source))
finally:
 source.write_bytes(before);example.unlink(missing_ok=True);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
