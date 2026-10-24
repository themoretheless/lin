from pathlib import Path
import subprocess,json,re,shutil,sys
p=Path(__file__).resolve().parent
with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run','--target-dir','/Users/themoretheless/Documents/Sources/lin/target'],stdout=log,stderr=subprocess.STDOUT)
(p/'build-status.json').write_text(json.dumps({'exit_code':r.returncode}))
if r.returncode:sys.exit(r.returncode)
exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'build.log').read_text())[1]
binary=Path('/tmp/lin-insert-profile-after-sharing');shutil.copyfile(exe,binary);binary.chmod(0o755)
Path('src/exec.rs').write_bytes((p/'before-exec.rs').read_bytes())
print('built and restored',flush=True)
