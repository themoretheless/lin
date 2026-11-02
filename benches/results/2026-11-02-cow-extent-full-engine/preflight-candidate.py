from pathlib import Path
import hashlib,json,os,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
assert not(p/'candidate-preflight.json').exists()and not(p/'candidate-smokes').exists(),'preserve prior attempt'
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
bench=root/'benches/compare.rs';original=bench.read_bytes();sources={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*')if f.is_file()};metadata=dict(performance_qualified=False,source_restored=False,compiler=subprocess.check_output(['rustc','-Vv'],text=True),smokes=[])
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2',LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None)
try:
 bench.write_bytes((p/'compare.rs').read_bytes());cmd=['cargo','bench','--bench','compare','--features','experimental-extent-wal','--no-run','--message-format=json-render-diagnostics'];metadata['build_command']=cmd
 with(p/'candidate-build.log').open('w')as log:r=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 artifacts=[]
 for line in(p/'candidate-build.log').read_text().splitlines():
  try:d=json.loads(line)
  except ValueError:continue
  if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='compare'and d.get('executable'):artifacts.append(d['executable'])
 assert len(artifacts)==1
 binary=Path('/tmp/lin-cow-extent-local-candidate');assert not binary.exists();shutil.copy2(artifacts[0],binary);metadata['binary']=dict(path=str(binary),sha256=sha(binary.read_bytes()))
finally:
 bench.write_bytes(original);metadata['source_restored']=bench.read_bytes()==original and sources=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*')if f.is_file()};(p/'candidate-preflight.json').write_text(json.dumps(metadata,indent=2)+'\n')
assert metadata['source_restored']
for case in json.loads((p/'cases.json').read_text()):
 dest=p/'candidate-smokes'/case.replace('/','_');dest.parent.mkdir(exist_ok=True)
 cmd=[str(binary),'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
 with dest.with_suffix('.log').open('w')as log:r=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 metadata['smokes'].append(dict(case=case,command=cmd,exit_code=r.returncode));(p/'candidate-preflight.json').write_text(json.dumps(metadata,indent=2)+'\n');assert r.returncode==0,case
 d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==1 and d['observations'][0]['case']==case and d['observations'][0]['operations']==1
assert len(metadata['smokes'])==67 and sha(binary.read_bytes())==metadata['binary']['sha256'];metadata['complete']=True;(p/'candidate-preflight.json').write_text(json.dumps(metadata,indent=2)+'\n');print('All 67 candidate functional smokes passed; not performance qualification')
