from pathlib import Path
import json,subprocess
p=Path(__file__).resolve().parent;status={}
for series in [3,4]:
 for mode in ['aa','ab']:
  name=f'serialguard-{mode}-s{series}'
  with (p/(name+'.log')).open('w') as log:r=subprocess.run(['python3',str(p/name/'run.py')],stdout=log,stderr=subprocess.STDOUT)
  status[name]=r.returncode;(p/'serialguard-status.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True)
  assert r.returncode==0
