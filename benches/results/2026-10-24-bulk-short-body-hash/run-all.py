from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
status={}
for stage in ["run-pairs.py","run-guards.py","run-rust-pairs.py"]:
 r=subprocess.run([sys.executable,str(p/stage)])
 status[stage]=r.returncode;(p/"measurement-status.json").write_text(json.dumps(status,indent=2));print(stage,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
