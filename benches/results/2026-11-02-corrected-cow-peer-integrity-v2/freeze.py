from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2]
m=json.loads((p/'input.json').read_text());sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
assert {str(f.relative_to(root)):sha(f)for f in(root/'src').rglob('*.rs')}==m['restore_source_sha256']
files=[root/n for n in ['.github/workflows/cow-peer-integrity-v2.yml','Cargo.toml','Cargo.lock','examples/peer_bench.rs','scripts/bench-peers.py','scripts/benchmark_source_integrity.py','scripts/test_benchmark_source_integrity.py','scripts/test-bench-peers.py','scripts/bench-peers-requirements.lock','benches/results/2026-11-01-corrected-cow-full-engine/source-inputs.json','benches/results/2026-11-02-mongo-ubuntu22-readiness/probe.py','benches/results/2026-11-02-mongo-ubuntu22-readiness-linux/status.json','benches/results/2026-11-02-mongo-ubuntu22-readiness-linux/run-metadata.json','lin-derive/Cargo.toml','lin-derive/src/lib.rs']]
files+=[p/n for n in ['build.py','run.py','audit.py','write-report.py','read/measure.py','native/measure.py','source-overlay.py','check-host.py','verify-processes.py','protocol.md','freeze.py','test-overlay.py']]
# Timed helpers are exact copies of the preceding corrected-source protocol.
old=root/'benches/results/2026-11-01-corrected-cow-peer-refresh'
for name in ['read/measure.py','native/measure.py']:assert(p/name).read_bytes()==(old/name).read_bytes()
m['files_sha256']={str(f.relative_to(root)):sha(f)for f in sorted(set(files))}
(p/'input.json').write_text(json.dumps(m,indent=2)+'\n');print('Frozen',len(files),'static source/build/harness/prerequisite files')
