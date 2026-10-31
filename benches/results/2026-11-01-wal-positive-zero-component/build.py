from pathlib import Path
import subprocess,json,hashlib,shutil,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;h=lambda b:hashlib.sha256(b).hexdigest()
files=[Path('src/persist.rs'),Path('src/persist/positive_zero.rs'),Path('benches/compare.rs')];before={f:f.read_bytes()for f in files}
manifest=json.loads((p/'input.json').read_text());tests=json.loads((p/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==manifest['source_sha256']
assert all(h(Path(f).read_bytes())==v for f,v in manifest['source_sha256'].items())
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=manifest['source_sha256'],compare_sha256=h((p/'compare.rs').read_bytes()),binaries={},instrumentation=False)
try:
 Path('benches/compare.rs').write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  Path('src/persist.rs').write_bytes((p/(role+'-persist.rs')).read_bytes())
  module=Path('src/persist/positive_zero.rs')
  if role=='baseline':module.unlink()
  else:module.write_bytes((p/'candidate-positive_zero.rs').read_bytes())
  role_source={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')}
  with(p/('build-'+role+'.log')).open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  executable=bench_executable((p/('build-'+role+'.log')).read_text(),'compare');dest=Path('/tmp/lin-wal-positive-zero-'+role);shutil.copy2(executable,dest)
  m['binaries'][role]=dict(path=str(dest),sha256=h(dest.read_bytes()),role_source_sha256=role_source)
finally:
 for f,b in before.items():f.write_bytes(b)
 m['source_restored']=all(f.read_bytes()==b for f,b in before.items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
assert m['source_restored']
