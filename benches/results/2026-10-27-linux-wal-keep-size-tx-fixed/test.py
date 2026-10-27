from pathlib import Path
import subprocess,json,hashlib,platform
p=Path(__file__).resolve().parent;f=Path('src/persist.rs');before=f.read_bytes();h=lambda path:hashlib.sha256(Path(path).read_bytes()).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(s):h(s)for s in Path('src').rglob('*.rs')},candidate_sha256=h(p/'candidate-persist.rs'),oracle_sha256=h(p/'oracle.rs'),platform=platform.platform(),linux_backend_executed=platform.system()=='Linux',performance_qualified=False)
try:
 f.write_text((p/'candidate-persist.rs').read_text()+'\n'+(p/'oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0,'inspect tests.log'
finally:
 f.write_bytes(before);m['source_restored']=all(h(s)==v for s,v in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n')
