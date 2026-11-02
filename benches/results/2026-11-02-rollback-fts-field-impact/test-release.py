from pathlib import Path
import hashlib
import json
import os
import platform
import re
import subprocess

p = Path(__file__).resolve().parent
root = p.parents[2]
hashes = {str(f.relative_to(root)): hashlib.sha256(f.read_bytes()).hexdigest() for f in (root/'src').glob('*.rs')}
assert hashes == json.loads((p/'source-before-tests.json').read_text())
env = dict(os.environ)
env.setdefault('CARGO_TARGET_DIR', str(root/'target/rollback-fts-field-impact'))
env.setdefault('CARGO_BUILD_JOBS', '2')
with (p/'full-release.log').open('w') as log:
    run = subprocess.run(['cargo', 'test', '--release'], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'full-release.log').read_text())
counts = [sum(int(r[i]) for r in rows) for i in range(3)]
restored = all(hashlib.sha256((root/f).read_bytes()).hexdigest() == h for f, h in hashes.items())
(p/'test-status.json').write_text(json.dumps(dict(exit_code=run.returncode, passed=counts[0], failed=counts[1], ignored=counts[2],
    source_sha256=hashes, source_restored=restored, platform=platform.platform(), rustc=subprocess.check_output(['rustc', '-Vv'], text=True),
    performance_qualification=False, full_objective_achieved=False), indent=2)+'\n')
assert run.returncode == 0 and restored and counts == [460, 0, 2]
