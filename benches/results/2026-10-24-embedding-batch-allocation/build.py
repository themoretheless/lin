from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent
example=Path("examples/embedding_batch_allocation.rs");assert not example.exists()
try:
 example.write_bytes((p/"gate.rs").read_bytes())
 with (p/"build.log").open("w") as log:r=subprocess.run(["cargo","build","--release","--example","embedding_batch_allocation"],stdout=log,stderr=subprocess.STDOUT)
 (p/"build-status.json").write_text(json.dumps({"exit_code":r.returncode}))
 if r.returncode:raise SystemExit(r.returncode)
 shutil.copy2("target/release/examples/embedding_batch_allocation","/tmp/lin-embedding-batch-allocation")
 (p/"binary-sha256.json").write_text(json.dumps({"sha256":hashlib.sha256(Path("/tmp/lin-embedding-batch-allocation").read_bytes()).hexdigest()}))
finally:example.unlink(missing_ok=True)
print("diagnostic built; temporary example removed",flush=True)
