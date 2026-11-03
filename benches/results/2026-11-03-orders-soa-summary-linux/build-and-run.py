from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,tarfile
assets=Path(__file__).resolve().parent;root=assets.parents[2];out=Path('/tmp/lin-orders-soa-summary-linux-20261103');out.mkdir(exist_ok=False)
inputs=json.loads((assets/'input.json').read_text());sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def verify():
 for name,digest in inputs['files_sha256'].items():assert sha(root/name)==digest,name
 assert {str(p.relative_to(root)) for base in ['src','lin-derive/src','tools/peer-worker/src'] for p in (root/base).rglob('*.rs')}==set(inputs['rust_source_files'])
verify();assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
refs={'candidate':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'baseline':inputs['baseline_runtime'],'production':inputs['production_runtime']};provenance={'refs':refs,'builds':{},'compiler':subprocess.check_output(['rustc','-Vv'],text=True)};bins={}
for role in ['candidate','baseline','production']:
 path=root if role=='candidate' else Path('/tmp/lin-summary-'+role+'-source-20261103')
 if path!=root:subprocess.run(['git','worktree','add','--detach',str(path),refs[role]],check=True)
 source={str(p.relative_to(path)):sha(p) for base in ['src','lin-derive/src','tools/peer-worker/src'] for p in (path/base).rglob('*.rs')}
 env=dict(os.environ,CARGO_TARGET_DIR=str(root/'target'),CARGO_BUILD_JOBS='2')
 with (out/f'build-{role}.txt').open('w') as log:subprocess.run(['cargo','build','--locked','--release','-p','lin-peer-worker'],cwd=path,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 binary=Path('/tmp/lin-summary-'+role+'-worker-20261103');shutil.copy2(root/'target/release/lin-peer-worker',binary);bins[role]=binary
 assert all(sha(path/name)==digest for name,digest in source.items())
 provenance['builds'][role]={'reference':refs[role],'source_sha256':source,'binary_sha256':sha(binary),'binary_bytes':binary.stat().st_size};(out/'build-provenance.json').write_text(json.dumps(provenance,indent=2)+'\n');verify()
command=[sys.executable,str(assets/'run.py'),'--output',str(out/'measurement'),'--baseline-binary',str(bins['baseline']),'--candidate-binary',str(bins['candidate']),'--production-binary',str(bins['production'])]
with (out/'measurement.txt').open('w') as log:status=subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT)
(out/'measurement-exit.json').write_text(json.dumps({'exit_code':status.returncode,'command':command},indent=2));assert status.returncode==0
verify();assert all(sha(bins[role])==record['binary_sha256'] for role,record in provenance['builds'].items())
q=json.loads((out/'measurement/decision.json').read_text());assert q['complete']; print(json.dumps(q),flush=True)
# The numeric gate is distinct from functional completion; retain all losses.
assert q['component_gate_passes'],'Numeric source gate failed; retain every original record'
