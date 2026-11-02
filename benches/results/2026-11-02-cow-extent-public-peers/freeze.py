from pathlib import Path
import hashlib,json
from source_manifest import manifest
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest();m=json.loads((p/'input.json').read_text());runtime=json.loads((p/'runtime-inputs.json').read_text())
assert m['runtime_reference']==runtime['runtime_reference']and m['features']==runtime['features']
for name,h in runtime['runtime_sha256'].items():assert sha(root/name)==h,name
files={root/n for n in manifest()};files.update(p.glob('*.py'));files.update([p/n for n in ['read/measure.py','native/measure.py','runtime-inputs.json','timed-helper-proof.json','protocol.md','workflow-path-audit.json','initial-workflow-path-failure.yml','local-worker-functional.tar.gz','local-worker-functional-manifest.json','local-worker-functional-audit.json']]);files.update(root/n for n in ['.github/workflows/cow-extent-public-peers.yml','scripts/bench-peers-requirements.lock','benches/results/2026-11-02-mongo-ubuntu22-readiness/probe.py','benches/results/2026-11-02-mongo-ubuntu22-readiness-linux/status.json','benches/results/2026-11-02-mongo-ubuntu22-readiness-linux/run-metadata.json'])
m['files_sha256']={str(f.relative_to(root)):sha(f)for f in sorted(files)};(p/'input.json').write_text(json.dumps(m,indent=2)+'\n');print('Frozen',len(files),'current-source/build/harness/prerequisite inputs')
