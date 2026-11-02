import pathlib, subprocess, hashlib, json, os
here = pathlib.Path(__file__).resolve().parent
root = here.parents[2]
src = root / 'src/exec.rs'
original = src.read_bytes()
patch = (here / 'supplement.rs').read_bytes()
env = dict(os.environ, CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101', CARGO_BUILD_JOBS='2')
try:
    src.write_bytes(original + b'\n' + patch)
    with (here / 'test.log').open('wb') as out:
        result = subprocess.run(['cargo','test','--release','rollback_after_257_distinct_posting_removals','--','--exact','exec::rollback_fts_real_fold_supplement::rollback_after_257_distinct_posting_removals','--nocapture'], cwd=root, env=env, stdout=out, stderr=subprocess.STDOUT)
finally:
    src.write_bytes(original)
assert src.read_bytes() == original
log = (here / 'test.log').read_text()
status = dict(returncode=result.returncode, original_sha256=hashlib.sha256(original).hexdigest(), supplement_sha256=hashlib.sha256(patch).hexdigest(), restored=True, passed=result.returncode == 0 and '1 passed; 0 failed' in log, performance_qualification=False)
(here / 'status.json').write_text(json.dumps(status, indent=2)+'\n')
assert status['passed'], log[-4000:]
print(json.dumps(status))
