from pathlib import Path
import subprocess,json,re,shutil,sys
p=Path(__file__).resolve().parent
label=sys.argv[1]
with (p/('build-'+label+'.log')).open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--target-dir','/Users/themoretheless/Documents/Sources/lin/target'],stdout=log,stderr=subprocess.STDOUT)
(p/('build-'+label+'-status.json')).write_text(json.dumps({'exit_code':r.returncode}))
if r.returncode:sys.exit(r.returncode)
exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/('build-'+label+'.log')).read_text())[1]
dst=Path('/tmp/lin-small-text-'+label);shutil.copyfile(exe,dst);dst.chmod(0o755)
print(label,'built',flush=True)
