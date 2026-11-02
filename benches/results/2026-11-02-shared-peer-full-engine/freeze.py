from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2]
m=json.loads((p/'source-inputs.json').read_text())
sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
assert {str(f.relative_to(root)):sha(f)for f in(root/'src').rglob('*.rs')}==m['restore_source_sha256']
names=['assignment.py','build.py','run.py','runtime.py','smoke.py','summarize.py','verify.py','qualify.py','test-protocol.py','test-prerequisite-audit.py','audit-prerequisites.py','source-inputs.json','cases.json','compare.rs','protocol.md','freeze.py','qualify-local.py','local-functional.tar.gz','local-functional-manifest.json','local-qualification.json']
files=[p/n for n in names]+[root/'.github/workflows/shared-peer-full-engine.yml',root/'scripts/benchmark_role_balance.py',root/'scripts/test-benchmark-role-balance.py',root/'scripts/cargo_bench_artifact.py',root/'benches/compare.rs']
files+=list((root/'src').rglob('*.rs'))
files+= [root/n for n in m['candidate']['build_sha256']]
manifest={str(f.relative_to(root)):sha(f)for f in sorted(set(files))}
(p/'frozen.json').write_text(json.dumps(manifest,indent=2)+'\n');print('Frozen',len(manifest),'immutable files')
