from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();ref=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','tests','lin-derive/src','tools/peer-worker/src'],cwd=root,text=True).splitlines()
files={n:sha(subprocess.check_output(['git','show',ref+':'+n],cwd=root))for n in names}
for n,h in files.items():assert sha((root/n).read_bytes())==h,n
extra=[p/n for n in ['build.py','measure.py','freeze.py','protocol.md','verify.py','audit-inputs.py','correctness.py']]+[root/n for n in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs','scripts/benchmark_role_balance.py','.github/workflows/embedding-modulus-linux-component.yml']]
for n in ['build.rs','rust-toolchain','rust-toolchain.toml']:
 if(root/n).is_file():extra.append(root/n)
if(root/'.cargo').exists():extra +=[f for f in(root/'.cargo').rglob('*')if f.is_file()]
files.update({str(f.relative_to(root)):sha(f.read_bytes())for f in extra});(p/'input.json').write_text(json.dumps(dict(runtime_reference=ref,baseline_reference=subprocess.check_output(['git','rev-parse','60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'],cwd=root,text=True).strip(),files_sha256=files,performance_qualified=False),indent=2)+'\n');print('Frozen',len(files),'inputs')
