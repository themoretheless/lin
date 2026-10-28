from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
with(p/'release-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
(p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode,source_sha256=m['source_sha256']),indent=2)+'\n');assert r.returncode==0
