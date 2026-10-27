from pathlib import Path
import hashlib,json,shutil,subprocess,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cargo_bench_artifact import bench_executable
p=Path(__file__).resolve().parent;files=[Path('src')/(n+'.rs')for n in ['exec','store','check','index']];before={f:f.read_bytes()for f in files};bench=Path('benches/compare.rs');bench_before=bench.read_bytes();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},original_compare_sha256=h(bench),compiled_compare_sha256=h(p/'compare.rs'),candidate_sources_sha256={str(f):h(p/('candidate-'+f.name))for f in files},baseline_sources_sha256={str(f):h(p/('baseline-'+f.name))for f in files},instrumentation=False,binaries={})
functional=json.loads((p/'test-status.json').read_text());assert functional['exit_code']==0 and functional['source_restored'] and functional['source_sha256']==m['source_sha256']
try:
 bench.write_bytes((p/'compare.rs').read_bytes())
 for role in ['baseline','candidate']:
  for f in files:f.write_bytes((p/('baseline-'+f.name)).read_bytes()if role=='baseline'else(p/('candidate-'+f.name)).read_bytes())
  with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role;exe=bench_executable((p/f'build-{role}.log').read_text(),'compare');dest=Path(f'/tmp/lin-cow-vnext-full-engine-{role}');shutil.copy2(exe,dest);m['binaries'][role]=dict(path=str(dest),sha256=h(dest))
finally:
 for f,data in before.items():f.write_bytes(data)
 bench.write_bytes(bench_before);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items())and h(bench)==m['original_compare_sha256'];(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
