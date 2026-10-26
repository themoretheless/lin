from pathlib import Path
import subprocess,json
r=Path(__file__).resolve().parent;status={}
for name in ['qualify.py','build-interleaved.py','final-checks.py','crash-resource.py','run-pairs.py','summarize.py','prepare-runners.py','run-interleaved-all.py','run-boundaries.py','native-controls.py']:
 with (r/(name+'.log')).open('w') as log:p=subprocess.run(['python3',str(r/name)],stdout=log,stderr=subprocess.STDOUT)
 status[name]=p.returncode;(r/'orchestration-status.json').write_text(json.dumps(status,indent=2));print(name,p.returncode,flush=True)
 if p.returncode:raise SystemExit(p.returncode)
