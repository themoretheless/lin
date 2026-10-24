from pathlib import Path
import subprocess,json,shutil,hashlib
p=Path(__file__).resolve().parent;f=Path("examples/wal_old_reader.rs");assert not f.exists()
try:
 f.write_bytes((p/"old-reader.rs").read_bytes())
 with (p/"old-reader-build.log").open("w") as log:r=subprocess.run(["cargo","build","--release","--example","wal_old_reader"],stdout=log,stderr=subprocess.STDOUT)
 (p/"old-reader-build-status.json").write_text(json.dumps({"exit_code":r.returncode}))
 if r.returncode:raise SystemExit(r.returncode)
 shutil.copy2("target/release/examples/wal_old_reader","/tmp/lin-old-reader-before-dictionary")
 (p/"old-reader-provenance.json").write_text(json.dumps({"source_commit":subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),"source_sha256":{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path("src").rglob("*.rs"))},"binary_sha256":hashlib.sha256(Path("/tmp/lin-old-reader-before-dictionary").read_bytes()).hexdigest()},indent=2))
finally:f.unlink(missing_ok=True)
print("old reader pinned",flush=True)
