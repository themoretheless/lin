from pathlib import Path
import subprocess,json,re,shutil,hashlib
p=Path(__file__).resolve().parent
status={'focused-tests':0}
for label,cmd in [('workspace',['cargo','test','--workspace']),('candidate-build',['cargo','bench','--bench','compare','--no-run'])]:
 with (p/(label+'.log')).open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 status[label]=r.returncode;(p/'checks.json').write_text(json.dumps(status,indent=2));print(label,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'candidate-build.log').read_text())[1];shutil.copy2(exe,'/tmp/lin-sparse-capture-candidate')
(p/'candidate-provenance.json').write_text(json.dumps({'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path('src').rglob('*.rs'))},'binary_sha256':{k:hashlib.sha256(Path('/tmp/lin-sparse-capture-'+k).read_bytes()).hexdigest() for k in ['baseline','candidate']}},indent=2))
