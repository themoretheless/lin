from pathlib import Path
import subprocess,json,sys
root=Path(__file__).resolve().parent
for name,cmd in [('tests',['cargo','test','--lib']),('build',['cargo','bench','--bench','compare','--no-run'])]:
 with (root/(name+'.log')).open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 (root/(name+'-status.json')).write_text(json.dumps({'exit_code':r.returncode}))
 print(name,r.returncode,flush=True)
 if r.returncode:sys.exit(r.returncode)
