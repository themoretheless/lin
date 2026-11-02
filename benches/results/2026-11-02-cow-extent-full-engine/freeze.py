from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest();m=json.loads((p/'source-inputs.json').read_text())
for n,h in m['runtime_and_tests_sha256'].items():assert sha(root/n)==h,n
names=['assignment.py','audit-harness.py','harness-audit.json','harness.patch','audit-baseline.py','audit-candidate.py','build.py','candidate-correctness.py','check-mutants.py','run.py','runtime.py','smoke.py','summarize.py','verify.py','qualify.py','test-protocol.py','source-inputs.json','cases.json','compare.rs','protocol.md','freeze.py','verify-inputs.py','preflight-candidate.py','local-functional.tar.gz','local-functional-manifest.json','local-functional-audit.json','local-correctness.tar.gz','local-correctness-manifest.json','local-correctness-audit.json']
files=[p/n for n in names]+[root/'.github/workflows/cow-extent-full-engine.yml',root/'scripts/benchmark_role_balance.py',root/'scripts/test-benchmark-role-balance.py',root/'scripts/cargo_bench_artifact.py',root/'benches/compare.rs',root/'benches/results/2026-11-02-shared-peer-full-engine/compare.rs',root/'benches/results/2026-11-02-shared-peer-full-engine/cases.json']+[root/n for n in m['runtime_and_tests_sha256']]+[root/n for n in m['candidate']['build_sha256']]
if(root/'.cargo').exists():files +=[f for f in(root/'.cargo').rglob('*')if f.is_file()]
for name in ['build.rs','rust-toolchain','rust-toolchain.toml']:
 if(root/name).is_file():files.append(root/name)
manifest={str(f.relative_to(root)):sha(f)for f in sorted(set(files))};(p/'frozen.json').write_text(json.dumps(manifest,indent=2)+'\n');print('Frozen',len(manifest),'inputs')
