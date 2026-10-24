from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
with (p/'workspace-tests.log').open('w') as log:r=subprocess.run(['cargo','test','--workspace','--target-dir','/Users/themoretheless/Documents/Sources/lin/target'],stdout=log,stderr=subprocess.STDOUT)
(p/'workspace-status.json').write_text(json.dumps({'exit_code':r.returncode}))
print('workspace',r.returncode,flush=True)
sys.exit(r.returncode)
