from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
inputs=json.loads((p/'source-input.json').read_text())
assert not Path('src/mutation_profile.rs').exists(),'Wait for test/build restoration'
for name,digest in inputs['source_sha256'].items():assert h(name)==digest,name
names=['prepare.py','profile.rs','instrumented-exec.rs','instrumented-persist.rs','instrumented-store.rs','instrumented-lib.rs','original-compare.rs','instrumented-compare.rs','source-input.json','test.py','build.py','smoke.py','run.py','analyze.py','verify.py','test-protocol.py','protocol.md','freeze.py']
source={str(f):h(f) for f in sorted(Path('src').rglob('*.rs'))}
build_paths=[Path('Cargo.toml'),Path('Cargo.lock'),Path('benches/compare.rs'),Path('scripts/cargo_bench_artifact.py'),Path('.github/workflows/corrected-durable-phase.yml')]
build_paths += sorted(Path('lin-derive').rglob('*.rs'))+sorted(Path('tests').rglob('*.rs'))
for optional in ['build.rs','lin-derive/Cargo.toml']:
 if Path(optional).exists():build_paths.append(Path(optional))
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256=source,build_inputs_sha256={str(f):h(f) for f in build_paths},files_sha256={name:h(p/name) for name in names},performance_measured=False,source_candidate_qualified=False,full_objective_achieved=False)
(p/'input-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
print('Frozen diagnostic inputs; no acceptance claim')
