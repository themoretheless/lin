from pathlib import Path
import subprocess,json,shutil
p=Path(__file__).resolve().parent
example=Path("examples/hash_cache_guard.rs")
assert not example.exists()
assert Path("src/exec.rs").read_bytes()==(p/"candidate-exec.rs").read_bytes()
statuses={}
try:
 example.write_bytes((p/"guard-worker.rs").read_bytes())
 for v in ["baseline","candidate"]:
  Path("src/exec.rs").write_bytes((p/("before-exec.rs" if v=="baseline" else "candidate-exec.rs")).read_bytes())
  with (p/("guard-build-"+v+".log")).open("w") as log:r=subprocess.run(["cargo","build","--release","--example","hash_cache_guard"],stdout=log,stderr=subprocess.STDOUT)
  statuses[v]=r.returncode;(p/"guard-build-status.json").write_text(json.dumps(statuses))
  if r.returncode:raise SystemExit(r.returncode)
  shutil.copy2("target/release/examples/hash_cache_guard","/tmp/lin-bulk-hash-guard-"+v)
finally:
 Path("src/exec.rs").write_bytes((p/"candidate-exec.rs").read_bytes())
 example.unlink(missing_ok=True)
print("guard binaries built; candidate source restored",flush=True)
