from pathlib import Path
import subprocess,json,re,shutil,hashlib
p=Path(__file__).resolve().parent
with (p/"baseline-build.log").open("w") as log:r=subprocess.run(["cargo","bench","--bench","compare","--no-run"],stdout=log,stderr=subprocess.STDOUT)
(p/"baseline-build-status.json").write_text(json.dumps({"exit_code":r.returncode}))
if r.returncode:raise SystemExit(r.returncode)
exe=re.search(r"Executable benches/compare.rs \(([^)]+)\)",(p/"baseline-build.log").read_text())[1]
shutil.copy2(exe,"/tmp/lin-dictionary-writer-baseline")
(p/"baseline-provenance.json").write_text(json.dumps({"source_commit":subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),"source_sha256":{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path("src").rglob("*.rs"))},"binary_sha256":hashlib.sha256(Path("/tmp/lin-dictionary-writer-baseline").read_bytes()).hexdigest()},indent=2))
print("baseline pinned",flush=True)
