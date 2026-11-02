from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
meta={}
for role,ref in [('baseline','625bc7180b37027d661afb87009464617c34de36'),('candidate','3fafb95894daebc24a38e3d2bf0cb338c141681f')]:
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','tests'],text=True,cwd=root).splitlines()
 meta[role]=dict(commit=ref,source_and_tests_sha256={n:sha(subprocess.check_output(['git','show',ref+':'+n],cwd=root))for n in names})
files=[root/n for n in ['Cargo.toml','Cargo.lock','.github/workflows/cow-shared-keys-linux.yml','scripts/benchmark_role_balance.py','scripts/test-benchmark-role-balance.py','lin-derive/Cargo.toml','lin-derive/src/lib.rs']]
files+=[p/n for n in ['build.py','run.py','verify.py','compare.rs','protocol.md','freeze.py']]
for n in ['build.rs','rust-toolchain','rust-toolchain.toml']:
 if(root/n).is_file():files.append(root/n)
if(root/'.cargo').exists():files+=[f for f in(root/'.cargo').rglob('*')if f.is_file()]
meta['files_sha256']={str(f.relative_to(root)):sha(f.read_bytes())for f in sorted(set(files))}
meta['harness_sha256']=sha((p/'compare.rs').read_bytes())
old=(root/'benches/results/2026-11-02-rollback-fts-field-impact-bench/compare.rs').read_bytes()
assert old.replace(b'(k.clone(),value)',b'(k.to_string(),value)')==(p/'compare.rs').read_bytes()
for n in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','lin-derive/src/lib.rs']:
 for role in ['baseline','candidate']:assert sha(subprocess.check_output(['git','show',meta[role]['commit']+':'+n],cwd=root))==meta['files_sha256'][n],(role,n)
(p/'input.json').write_text(json.dumps(meta,indent=2)+'\n');print('frozen',len(files),'inputs, exact source/test pair and identical adapted harness')
