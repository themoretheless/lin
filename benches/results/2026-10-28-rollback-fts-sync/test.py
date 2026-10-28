from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
with(p/'release-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',(p/'release-tests.log').read_text())
s=dict(exit_code=r.returncode,source_sha256=m['source_sha256'],release_passed=sum(int(c[0])for c in counts),release_ignored=sum(int(c[2])for c in counts),performance_qualified=False)
(p/'test-status.json').write_text(json.dumps(s,indent=2)+'\n');assert r.returncode==0 and s['release_passed']==454 and s['release_ignored']==2
