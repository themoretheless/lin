from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();ref='85dd9af71b1cee0ea1f909facdfc2d8149afb375'
names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','tests'],text=True,cwd=root).splitlines()
source={n:sha(subprocess.check_output(['git','show',ref+':'+n],cwd=root))for n in names}
assert source=={str(f.relative_to(root)):sha(f.read_bytes())for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()}
files=[p/n for n in ['build.py','run.py','verify.py','compare.rs','protocol.md','freeze.py','test-audit.py']]
files+=[root/n for n in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','lin-derive/src/lib.rs','scripts/benchmark_role_balance.py','scripts/test-benchmark-role-balance.py','.github/workflows/extent-wal-component.yml']]
for n in ['build.rs','rust-toolchain','rust-toolchain.toml']:
 if(root/n).is_file():files.append(root/n)
if(root/'.cargo').exists():files+=[f for f in(root/'.cargo').rglob('*')if f.is_file()]
metadata=dict(runtime_reference=ref,source_and_tests_sha256=source,files_sha256={str(f.relative_to(root)):sha(f.read_bytes())for f in sorted(set(files))})
(p/'input.json').write_text(json.dumps(metadata,indent=2)+'\n')
print('Frozen',len(files),'inputs and complete runtime/test tree')
