from pathlib import Path
import subprocess,json
r=Path(__file__).resolve().parent;out={}
for name,args in [('postguard-focused',['cargo','test','--lib','preencoded_insert_tests']),('postguard-workspace',['cargo','test','--workspace']),('postguard-release',['python3',str(r/'final-checks.py')])]:
 with (r/(name+'.log')).open('w') as log:p=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 out[name]=p.returncode;(r/'postguard-checks.json').write_text(json.dumps(out,indent=2));print(name,p.returncode,flush=True)
 if p.returncode:raise SystemExit(p.returncode)
