from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();ref='60e64559e'
ref=subprocess.check_output(['git','rev-parse',ref],cwd=root,text=True).strip();names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','lin-derive/src','tools/peer-worker/src'],cwd=root,text=True).splitlines()
runtime={n:sha(subprocess.check_output(['git','show',ref+':'+n],cwd=root))for n in names};assert all(sha((root/n).read_bytes())==h for n,h in runtime.items())
files=[p/n for n in ['run.py','freeze.py','protocol.md']]+[root/n for n in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs','.github/workflows/native-insert-cpu-profile.yml']]
for n in ['build.rs','rust-toolchain','rust-toolchain.toml']:
 if(root/n).is_file():files.append(root/n)
if(root/'.cargo').exists():files +=[f for f in(root/'.cargo').rglob('*')if f.is_file()]
(p/'input.json').write_text(json.dumps(dict(runtime_reference=ref,runtime_sha256=runtime,files_sha256={str(f.relative_to(root)):sha(f.read_bytes())for f in files},profiling_only=True,performance_qualified=False),indent=2)+'\n');print('Frozen native-worker profile inputs')
