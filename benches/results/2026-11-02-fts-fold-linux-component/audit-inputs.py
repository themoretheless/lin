from pathlib import Path
import hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];ref='9e15eca1a';base='60e64559e';sha=lambda b:hashlib.sha256(b).hexdigest()
changed=subprocess.check_output(['git','diff','--name-only',base,ref,'--','src'],cwd=root,text=True).splitlines();assert changed==['src/fts.rs'],changed
for folder in ['lin-derive/src','tools/peer-worker/src']:
 assert not subprocess.check_output(['git','diff','--name-only',base,ref,'--',folder],cwd=root,text=True).strip()
for name in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs']:
 assert subprocess.check_output(['git','show',base+':'+name],cwd=root)==subprocess.check_output(['git','show',ref+':'+name],cwd=root),name
workflow=(root/'.github/workflows/fts-fold-linux-component.yml').read_text();prefix=str(p.relative_to(root));paths=re.findall(r'benches/results/[A-Za-z0-9_./-]+',workflow);assert paths and all(x.rstrip('/')==prefix or x.startswith(prefix+'/')for x in paths)
for name in paths:
 if name.endswith('.py'):assert(root/name).is_file(),name
assert "'--features','lin/experimental-extent-wal'"in(p/'build.py').read_text()
(p/'source-delta.json').write_text(json.dumps(dict(baseline_reference=subprocess.check_output(['git','rev-parse',base],cwd=root,text=True).strip(),candidate_reference=subprocess.check_output(['git','rev-parse',ref],cwd=root,text=True).strip(),changed_production_source=changed,worker_derive_and_build_inputs_identical=True,cargo_feature_selection='lin/experimental-extent-wal',performance_qualified=False),indent=2)+'\n');print('PASS exact FTS-only source delta, same worker/build inputs and all workflow paths')
