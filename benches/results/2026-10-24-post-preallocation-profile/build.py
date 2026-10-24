from pathlib import Path
import subprocess,json,re,shutil,hashlib
p=Path(__file__).resolve().parent
try:
 with (p/"build.log").open("w") as log:r=subprocess.run(["cargo","bench","--bench","compare","--no-run"],stdout=log,stderr=subprocess.STDOUT)
 (p/"build-status.json").write_text(json.dumps({"exit_code":r.returncode}))
 if r.returncode:raise SystemExit(r.returncode)
 exe=re.search(r"Executable benches/compare.rs \(([^)]+)\)",(p/"build.log").read_text())[1]
 shutil.copy2(exe,"/tmp/lin-post-preallocation-profile")
 (p/"binary-sha256.json").write_text(json.dumps({"sha256":hashlib.sha256(Path("/tmp/lin-post-preallocation-profile").read_bytes()).hexdigest()}))
finally:Path("src/persist.rs").write_bytes((p/"before-persist.rs").read_bytes())
print("profile built and source restored",flush=True)
