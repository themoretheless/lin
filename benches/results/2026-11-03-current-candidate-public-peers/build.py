from pathlib import Path
import hashlib,json,os,subprocess,shutil,time
from source_manifest import manifest
assets=Path(__file__).resolve().parent;root=assets.parents[2];out=Path('/tmp/lin-current-candidate-public-peers-20261103');out.mkdir(exist_ok=False)
inputs=json.loads((assets/'input.json').read_text());before=manifest()
assert not any(k.startswith('CARGO_PROFILE_') for k in os.environ) and not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
for name,digest in inputs['runtime_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',inputs['runtime_reference']+':'+name])).hexdigest()==digest
command=['cargo','build','--locked','--release','-p','lin-peer-worker']
attempts=[]
env=dict(os.environ,CARGO_NET_RETRY='5',CARGO_HTTP_MULTIPLEXING='false')
for attempt in range(1,4):
 log=out/f'build-attempt-{attempt}.txt'
 with log.open('w') as f:result=subprocess.run(command,cwd=root,stdout=f,stderr=subprocess.STDOUT,env=env)
 attempts.append({'attempt':attempt,'exit_code':result.returncode,'log':log.name});(out/'build-attempts.json').write_text(json.dumps(attempts,indent=2)+'\n')
 if result.returncode==0:
  shutil.copy2(log,out/'build.txt');break
 text=log.read_text();network=any(x in text for x in ['unable to update registry','failed to download','failed to fetch'])
 assert network and 'could not compile' not in text and 'error[E' not in text and attempt<3,'Build failed: preserve diagnostic; do not retry compilation or measurements'
 time.sleep(3)
else:raise AssertionError('No successful ordinary worker build')
binary=Path('/tmp/lin-current-candidate-public-peer-built-20261103');assert not binary.exists();shutil.copy2(root/'target/release/lin-peer-worker',binary)
m={'runtime_reference':inputs['runtime_reference'],'features':['default'],'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':before,'source_unchanged':manifest()==before,'build_command':command,'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'build_attempts':attempts,'network_transport':{'CARGO_NET_RETRY':'5','CARGO_HTTP_MULTIPLEXING':'false'},'binary':{'path':str(binary),'bytes':binary.stat().st_size,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}}
assert m['source_unchanged'];(out/'build-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
(out/'input.json').write_text(json.dumps(inputs,indent=2)+'\n')
