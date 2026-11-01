from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text())
for key in ['source_sha256','tests_sha256']:
 assert all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m[key].items())
with(p/'full-release.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
c=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',(p/'full-release.log').read_text())
s=dict(exit_code=r.returncode,release_passed=sum(int(x[0])for x in c),release_ignored=sum(int(x[2])for x in c),source_sha256=m['source_sha256'],tests_sha256=m['tests_sha256'],compiler=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False)
(p/'test-status.json').write_text(json.dumps(s,indent=2)+'\n');assert r.returncode==0 and s['release_passed']==441 and s['release_ignored']==2
