"""Create real WAL fixtures and verify them with an exported production reader."""
from pathlib import Path
import subprocess,json,hashlib,tempfile,os
ROOT=Path.cwd();p=Path(__file__).resolve().parent;ref='04749994b9ca7a090476b95401f0f7166d8b870a'
source={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')}
snapshot=Path(tempfile.mkdtemp(prefix='lin-wal-positive-zero-legacy-'))
status=dict(legacy_reference=ref,snapshot=str(snapshot),source_sha256=source,legacy_source_sha256={},performance_qualified=False)
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-cow-vnext-target')
current_example=ROOT/'examples/wal_positive_zero_fixtures.rs';assert not current_example.exists()
legacy_example=snapshot/'examples/wal_positive_zero_reader.rs'
def build(cwd,name,label):
 with(p/(label+'-build.log')).open('w')as log:r=subprocess.run(['cargo','build','--release','--locked','--example',name,'--message-format=json-render-diagnostics'],cwd=cwd,env=env,stdout=log,stderr=subprocess.STDOUT)
 status[label+'_build_exit']=r.returncode;assert r.returncode==0,label
 paths=[]
 for line in(p/(label+'-build.log')).read_text().splitlines():
  try:q=json.loads(line)
  except ValueError:continue
  if q.get('reason')=='compiler-artifact'and q.get('target',{}).get('name')==name and q.get('executable'):paths.append(q['executable'])
 assert len(paths)==1;status[label+'_binary_sha256']=hashlib.sha256(Path(paths[0]).read_bytes()).hexdigest();return paths[0]
try:
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','lin-derive','tools/peer-worker','Cargo.toml','Cargo.lock','benches/compare.rs'],text=True).splitlines()
 for name in names:
  data=subprocess.check_output(['git','show',ref+':'+name]);dest=snapshot/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data);status['legacy_source_sha256'][name]=hashlib.sha256(data).hexdigest()
 current_example.parent.mkdir(exist_ok=True);current_example.write_bytes((p/'fixture-writer.rs').read_bytes())
 binary=build(ROOT,'wal_positive_zero_fixtures','current')
 with(p/'current-outcomes.jsonl').open('w')as out,(p/'current-runtime.log').open('w')as log:r=subprocess.run([binary,str(p/'fixtures')],stdout=out,stderr=log)
 status['current_runtime_exit']=r.returncode;assert r.returncode==0
 legacy_example.parent.mkdir(exist_ok=True);legacy_example.write_bytes((p/'legacy-reader.rs').read_bytes())
 binary=build(snapshot,'wal_positive_zero_reader','legacy')
 with(p/'legacy-outcomes.jsonl').open('w')as out,(p/'legacy-runtime.log').open('w')as log:r=subprocess.run([binary,str(p/'fixtures')],stdout=out,stderr=log)
 status['legacy_runtime_exit']=r.returncode;assert r.returncode==0
 rows=[json.loads(s)for s in(p/'legacy-outcomes.jsonl').read_text().splitlines()];assert len(rows)==8 and all(x['legacy_full_record_bits_verified']and x['legacy_replay_verified']for x in rows)
 status['legacy_cases_verified']=8
 status['fixtures_sha256']={f.name:hashlib.sha256(f.read_bytes()).hexdigest()for f in(p/'fixtures').iterdir()if f.is_file()}
finally:
 current_example.unlink(missing_ok=True);legacy_example.unlink(missing_ok=True)
 status['current_sources_unchanged']=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in source.items())
 status['legacy_sources_unchanged']=all(hashlib.sha256((snapshot/f).read_bytes()).hexdigest()==h for f,h in status['legacy_source_sha256'].items())
 (p/'legacy-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['current_sources_unchanged']and status['legacy_sources_unchanged']
print('8 bounded production-reader WAL cases passed; no timing or power-loss qualification')
