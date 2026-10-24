from pathlib import Path
import subprocess,json,re,shutil,hashlib
p=Path(__file__).resolve().parent
assert Path("src/persist.rs").read_bytes()==(p/"candidate-persist.rs").read_bytes()
status={}
with (p/"workspace.log").open("w") as log:r=subprocess.run(["cargo","test","--workspace"],stdout=log,stderr=subprocess.STDOUT)
status["workspace"]=r.returncode;(p/"candidate-checks.json").write_text(json.dumps(status));print("workspace",r.returncode,flush=True)
if r.returncode:raise SystemExit(r.returncode)
f=Path("examples/dictionary_crash_worker.rs");assert not f.exists()
try:
 f.write_bytes((p/"crash-worker.rs").read_bytes())
 with (p/"crash-worker-build.log").open("w") as log:r=subprocess.run(["cargo","build","--release","--example","dictionary_crash_worker"],stdout=log,stderr=subprocess.STDOUT)
 status["crash-worker-build"]=r.returncode;(p/"candidate-checks.json").write_text(json.dumps(status))
 if r.returncode:raise SystemExit(r.returncode)
 shutil.copy2("target/release/examples/dictionary_crash_worker","/tmp/lin-dictionary-crash-worker-final")
finally:f.unlink(missing_ok=True)
with (p/"candidate-build.log").open("w") as log:r=subprocess.run(["cargo","bench","--bench","compare","--no-run"],stdout=log,stderr=subprocess.STDOUT)
status["candidate-build"]=r.returncode;(p/"candidate-checks.json").write_text(json.dumps(status));print("candidate-build",r.returncode,flush=True)
if r.returncode:raise SystemExit(r.returncode)
exe=re.search(r"Executable benches/compare.rs \(([^)]+)\)",(p/"candidate-build.log").read_text())[1];shutil.copy2(exe,"/tmp/lin-dictionary-writer-candidate")
(p/"candidate-provenance.json").write_text(json.dumps({"source_sha256":{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path("src").rglob("*.rs"))},"binary_sha256":{k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in {"baseline":"/tmp/lin-dictionary-writer-baseline","candidate":"/tmp/lin-dictionary-writer-candidate","crash_worker":"/tmp/lin-dictionary-crash-worker-final"}.items()}},indent=2))
