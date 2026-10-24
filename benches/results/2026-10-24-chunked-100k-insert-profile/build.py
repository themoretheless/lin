from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent
try:
 with (p/"build.log").open("w") as log:r=subprocess.run(["cargo","build","--release","--example","peer_bench"],stdout=log,stderr=subprocess.STDOUT)
 (p/"build-status.json").write_text(json.dumps({"exit_code":r.returncode}))
 if r.returncode:raise SystemExit(r.returncode)
 shutil.copy2("target/release/examples/peer_bench","/tmp/lin-chunked-100k-profile")
 (p/"binary-sha256.json").write_text(json.dumps({"sha256":hashlib.sha256(Path("/tmp/lin-chunked-100k-profile").read_bytes()).hexdigest()}))
finally:Path("src/exec.rs").write_bytes((p/"before-exec.rs").read_bytes())
print("built and source restored",flush=True)
