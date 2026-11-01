from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import time

repo = Path.cwd()
sys.path.insert(0, str(repo/'scripts'))
from benchmark_role_balance import schedule
sys.path.insert(0, str(Path(__file__).resolve().parent.parent/'2026-11-01-corrected-cow-full-engine'))
from runtime import capture, env_for

p = Path(__file__).resolve().parent
full = p.parent/'2026-11-01-corrected-cow-full-engine'
read = lambda f: json.loads(f.read_text())
digest = lambda f: hashlib.sha256(f.read_bytes()).hexdigest()
frozen = read(p/'frozen.json')
assert all(digest(repo/name) == h for name, h in frozen.items())
meta = read(full/'provenance.json')
assert meta['source_restored'] and not meta['instrumentation']
assert all(digest(repo/name) == h for name, h in meta['source_sha256'].items())
assert digest(repo/'benches/compare.rs') == meta['original_compare_sha256']
smokes = read(full/'local-smoke-status.json')
assert len(smokes['runs']) == 134 and all(r['exit_code'] == 0 for r in smokes['runs'])
roles = ['baseline_a', 'candidate_a', 'baseline_b', 'candidate_b', 'baseline_c', 'candidate_c']
cases = read(p/'cases.json')
planned = schedule(cases, roles)
assert len(planned) == 216
capture(p)
(p/'provenance.json').write_text(json.dumps(meta, indent=2)+'\n')
root = p/'measurement'
root.mkdir(exist_ok=False)
records = []
for item in planned:
    rnd, case, role = item['round'], item['workload'], item['role']
    source_role = role.split('_')[0]
    binary = meta['binaries'][source_role]
    assert digest(Path(binary['path'])) == binary['sha256']
    name = f'{rnd:02}-{role}-'+case.replace('/', '_')
    dest = root/name
    command = ['/usr/bin/time', '-v', '-o', str(root/(name+'-resource.txt')), '--', binary['path'],
               '--filter', case, '--samples', '8', '--max-iterations', '1', '--warmup-ms', '0',
               '--sample-ms', '1', '--output', str(dest)]
    started = time.time()
    with (root/(name+'.log')).open('w') as log:
        result = subprocess.run(command, env=env_for(source_role), stdout=log, stderr=subprocess.STDOUT)
    records.append(dict(round=rnd, case=case, role=role, source_role=source_role,
                        binary_sha256=binary['sha256'], command=command, output=name,
                        exit_code=result.returncode, started_unix=started, elapsed_seconds=time.time()-started))
    (root/'process-status.json').write_text(json.dumps(records, indent=2)+'\n')
    assert result.returncode == 0
    data = read(dest/'run.json')
    assert data['status'] == 'complete' and len(data['observations']) == 8
    print(rnd, role, case, 'complete', flush=True)
assert all(digest(Path(b['path'])) == b['sha256'] for b in meta['binaries'].values())
assert all(digest(repo/name) == h for name, h in meta['source_sha256'].items())
