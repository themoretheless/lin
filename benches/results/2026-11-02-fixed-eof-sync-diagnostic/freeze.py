from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2]
files=[p/n for n in ['probe.rs','run.py','audit.py','protocol.md','freeze.py','test-audit.py']]
files += [root/n for n in ['scripts/benchmark_role_balance.py','scripts/test-benchmark-role-balance.py','.github/workflows/fixed-eof-sync-diagnostic.yml']]
(p/'input.json').write_text(json.dumps(dict(files_sha256={str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest()for f in files},production_runtime_changes=False,performance_qualification=False),indent=2)+'\n')
print('Frozen',len(files),'standalone diagnostic inputs')
