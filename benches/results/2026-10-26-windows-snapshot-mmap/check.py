from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;status={}
for name,args in [('gpu-library',['cargo','check','--locked','--features','gpu','--lib','--bin','lin']),('cpu-library',['cargo','check','--locked','--no-default-features','--lib','--bin','lin']),('snapshot-tests',['cargo','test','--test','persist','--test','snapshot_tests'])]:
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 status[name]=r.returncode;(p/'checks.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True);assert r.returncode==0
