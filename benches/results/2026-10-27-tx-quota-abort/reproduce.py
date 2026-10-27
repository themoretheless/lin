from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;target=Path('tests/tx_quota_abort_probe.rs');assert not target.exists();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},regression_sha256=h(p/'regression.rs'),production_source_unchanged=True)
try:
 target.write_bytes((p/'regression.rs').read_bytes())
 with(p/'baseline.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--test','tx_quota_abort_probe','--color','never','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'baseline.log').read_text();m['exit_code']=r.returncode;assert r.returncode!=0 and 'test result: FAILED'in text and 'error[E'not in text
finally:
 target.unlink(missing_ok=True);m['source_restored']=all(h(f)==d for f,d in m['source_sha256'].items());(p/'baseline-status.json').write_text(json.dumps(m,indent=2)+'\n')
