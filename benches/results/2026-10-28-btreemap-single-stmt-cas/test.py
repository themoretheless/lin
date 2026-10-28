from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input-provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==d for f,d in m['source_sha256'].items())
with(p/'release-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
s=(p/'release-tests.log').read_text();counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',s)]
status=dict(exit_code=r.returncode,passed=sum(x[0]for x in counts),ignored=sum(x[2]for x in counts),source_sha256=m['source_sha256'],row_alias_contract_passed='row_keeps_exact_btreemap_type_identity_after_cas_update ... ok'in s,source_restored=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==d for f,d in m['source_sha256'].items()),performance_qualified=False)
(p/'test-status.json').write_text(json.dumps(status,indent=2)+'\n');assert status['exit_code']==0 and status['passed']==436 and status['ignored']==2 and status['source_restored']and status['row_alias_contract_passed']
