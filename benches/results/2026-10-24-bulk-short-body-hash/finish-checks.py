from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
status={}
for name,cmd in [("rss",[sys.executable,str(p/"rss.py")]),("workspace",["cargo","test","--workspace"])]:
 with (p/(name+"-checks.log")).open("w") as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 status[name]=r.returncode;(p/"finish-status.json").write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
