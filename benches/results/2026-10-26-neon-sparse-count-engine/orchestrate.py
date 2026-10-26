from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;status={}
for name in ['qualify.py','build-interleaved.py','final-checks.py','crash-resource.py','run-pairs.py','summarize.py','prepare-runners.py','run-interleaved-all.py','run-boundaries.py','native-controls.py']:
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(['python3',str(p/name)],stdout=log,stderr=subprocess.STDOUT)
 status[name]=r.returncode;(p/'orchestration-status.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True);assert r.returncode==0
