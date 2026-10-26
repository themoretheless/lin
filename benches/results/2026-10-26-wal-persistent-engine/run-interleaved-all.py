from pathlib import Path
import subprocess,json
r=Path(__file__).resolve().parent;status={}
for mode in ['aa','ab','aa-s2','ab-s2']:
 with (r/f'interleaved-{mode}.log').open('w') as log:p=subprocess.run(['python3',str(r/f'interleaved-{mode}/run.py')],stdout=log,stderr=subprocess.STDOUT)
 status[mode]=p.returncode;(r/'interleaved-status.json').write_text(json.dumps(status,indent=2));print(mode,p.returncode,flush=True);assert p.returncode==0
