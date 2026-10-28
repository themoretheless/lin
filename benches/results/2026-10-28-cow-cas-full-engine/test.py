from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f) for f in Path('src').rglob('*.rs')},performance_qualified=False)
with(p/'full-tests.log').open('w') as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
m['exit_code']=r.returncode;m['source_restored']=all(h(f)==d for f,d in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n');assert r.returncode==0 and m['source_restored']
