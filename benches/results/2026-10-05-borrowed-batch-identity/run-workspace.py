from pathlib import Path
import subprocess,json,sys
root=Path(__file__).resolve().parent
with (root/'workspace-tests.log').open('w') as log:
 result=subprocess.run(['cargo','test','--workspace'],stdout=log,stderr=subprocess.STDOUT)
(root/'workspace-status.json').write_text(json.dumps({'exit_code':result.returncode},indent=2))
print('workspace exit',result.returncode,flush=True)
sys.exit(result.returncode)
