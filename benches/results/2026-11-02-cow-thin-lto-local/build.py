from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
sys.path.insert(0,str(root/'scripts'))
from cargo_bench_artifact import bench_executable
sha=lambda b:hashlib.sha256(b).hexdigest()
old=root/'benches/results/2026-11-02-rollback-fts-field-impact-bench'
inputs=json.loads((old/'input.json').read_text());prior=json.loads((old/'provenance.json').read_text())
baseline=inputs['baseline'];assert baseline['commit'].startswith('625bc7180')
b=prior['binaries']['baseline'];assert b['source_sha256']==baseline['source_sha256']
assert sha(Path(b['path']).read_bytes())==b['sha256']
for name,h in inputs['build_sha256'].items():assert sha((root/name).read_bytes())==h
sources={name:subprocess.check_output(['git','show',baseline['commit']+':'+name],cwd=root) for name in baseline['source_sha256']}
assert {name:sha(data)for name,data in sources.items()}==baseline['source_sha256']
original={f:f.read_bytes()for f in (root/'src').rglob('*.rs')};bench=root/'benches/compare.rs';oldbench=bench.read_bytes()
harness=(old/'compare.rs').read_bytes();assert sha(harness)=='59b8acb6b9157d6b5fde56332fb35b0f2e0ea4728eb7f6a6b8c0c53c14b58508'
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2')
assert not any(k.startswith('CARGO_PROFILE_') for k in os.environ)
assert not env.get('RUSTFLAGS')and not env.get('CARGO_ENCODED_RUSTFLAGS')
command=['cargo','bench','--bench','compare','--no-run','--color','never','--verbose','--config','profile.release.lto="thin"']
meta=dict(source_commit=baseline['commit'],source_sha256=baseline['source_sha256'],build_sha256=inputs['build_sha256'],harness_sha256=sha(harness),baseline=b,candidate_command=command,cargo_target_dir=env['CARGO_TARGET_DIR'],rustc=subprocess.check_output(['rustc','-Vv'],text=True),runtime_changes=False,performance_qualification=False,source_restored=False)
try:
 for f in (root/'src').rglob('*.rs'):
  if str(f.relative_to(root))not in sources:f.unlink()
 for name,data in sources.items():(root/name).write_bytes(data)
 bench.write_bytes(harness)
 with(p/'build.log').open('w')as log:run=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 assert run.returncode==0
 text=(p/'build.log').read_text()
 assert any('--crate-name compare 'in line and '-C lto=thin'in line for line in text.splitlines()), 'actual compare ThinLTO compiler flag required'
 try: artifact=bench_executable(text,'compare')
 except ValueError:
  matches=re.findall(r'^\s*Executable `([^`]+)`\s*$',text,re.M);assert len(matches)==1
  artifact=matches[0]
 exe=Path(artifact).resolve();assert exe.is_relative_to(Path(env['CARGO_TARGET_DIR']).resolve())
 dest=Path('/tmp/lin-cow-thin-lto-candidate');assert not dest.exists(),'retain earlier executable'
 shutil.copy2(exe,dest)
 meta['candidate']=dict(path=str(dest),sha256=sha(dest.read_bytes()),lto='thin')
 assert meta['candidate']['sha256']!=b['sha256']
 peer=Path('/tmp/lin-cow-thin-lto-external');assert not peer.exists(),'retain earlier external executable'
 shutil.copy2(b['path'],peer);assert sha(peer.read_bytes())==b['sha256']
 meta['peer_binary']=dict(path=str(peer),sha256=b['sha256'],source_commit=baseline['commit'],compiled_compare_sha256=sha(harness))
finally:
 for f in(root/'src').rglob('*.rs'):
  if f not in original:f.unlink()
 for f,data in original.items():f.write_bytes(data)
 bench.write_bytes(oldbench)
 meta['source_restored']=all(f.read_bytes()==data for f,data in original.items())and bench.read_bytes()==oldbench
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored'];print(json.dumps(dict(built=True,source_restored=True,performance_qualification=False)))
