from pathlib import Path
import subprocess,json,re,shutil
p=Path(__file__).resolve().parent
f=Path('src/exec.rs');assert f.read_bytes()==(p/'before-exec.rs').read_bytes()
try:
 f.write_bytes((p/'instrumented-exec.rs').read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','bench','--bench','compare','--no-run'],stdout=log,stderr=subprocess.STDOUT)
 (p/'build-status.json').write_text(json.dumps({'exit_code':r.returncode}));assert r.returncode==0
 exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'build.log').read_text())[1];shutil.copy2(exe,'/tmp/lin-current-insert-stage-profile')
finally:f.write_bytes((p/'before-exec.rs').read_bytes())
print('built and source restored',flush=True)
