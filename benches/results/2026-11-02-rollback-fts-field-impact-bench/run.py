from pathlib import Path
import hashlib
import json
import os
import platform
import subprocess
import sys
import time
sys.path.insert(0, str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule

p = Path(__file__).resolve().parent
digest = lambda f: hashlib.sha256(f.read_bytes()).hexdigest()
frozen = json.loads((p/'frozen.json').read_text())
assert all(digest(Path(f)) == h for f, h in frozen.items())
meta = json.loads((p/'provenance.json').read_text())
assert meta['source_restored'] and not meta['instrumentation']
cases = json.loads((p/'cases.json').read_text())
assert len(cases) == len(set(cases)) == 20
plan = schedule([dict(id=c) for c in cases], ['baseline', 'control', 'candidate'])
assert len(plan) == 720 and not (p/'process-status.json').exists()
assert platform.system() == 'Linux' and platform.machine() == 'x86_64'
assert not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT')
env = dict(os.environ, LIN_BENCH_SKIP_MYSQL='1', LIN_BENCH_SKIP_PG='1', LC_ALL='C')
for name in ['LIN_BENCH_MIXED_CASE', 'LIN_COLUMN_BATCH_FIXTURE']: env.pop(name, None)
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(), rustc=subprocess.check_output(['rustc', '-Vv'], text=True), same_runtime_all_roles=True, memory_measured=False), indent=2)+'\n')
smokes = json.loads((p/'qualification-smoke-status.json').read_text())
assert len(smokes) == 40 and all(r['exit'] == 0 for r in smokes)
assert {(r['role'], r['case']) for r in smokes} == {(role, c) for role in ['baseline', 'candidate'] for c in cases}
records = []
for item in plan:
    rnd, role, case = item['round'], item['role'], item['workload']
    binary = meta['binaries']['baseline' if role == 'control' else role]
    assert digest(Path(binary['path'])) == binary['sha256']
    dest = p/f'{rnd:02}-{role}'/case.replace('/', '_'); dest.parent.mkdir(exist_ok=True)
    command = [binary['path'], '--filter', case, '--samples', '8', '--max-iterations', '1', '--warmup-ms', '0', '--sample-ms', '1', '--output', str(dest)]
    start = time.time()
    with dest.with_suffix('.log').open('w') as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    records.append(dict(round=rnd, mode=role, case=case, command=command, exit_code=result.returncode,
                        binary_sha256=binary['sha256'], elapsed_seconds=time.time()-start))
    (p/'process-status.json').write_text(json.dumps(records, indent=2)+'\n')
    assert result.returncode == 0
    print(rnd, role, case, 'complete', flush=True)
assert all(digest(Path(f)) == h for f, h in meta['source_sha256'].items())
