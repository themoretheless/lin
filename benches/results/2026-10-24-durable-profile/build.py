from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--target-dir','/Users/themoretheless/Documents/Sources/lin/target'],stdout=log,stderr=subprocess.STDOUT)
(p/'build-status.json').write_text(json.dumps({'exit_code':r.returncode}))
print('build',r.returncode,flush=True)
sys.exit(r.returncode)
