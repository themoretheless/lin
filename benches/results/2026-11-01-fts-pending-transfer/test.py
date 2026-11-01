from pathlib import Path
import subprocess,json,re,hashlib
p=Path(__file__).resolve().parent
with(p/'full-release.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
text=(p/'full-release.log').read_text();counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)];totals=[sum(x[i]for x in counts)for i in range(3)]
assert r.returncode==0 and totals==[459,0,2],totals
assert 'singleton_pending_transfer_reuses_storage_and_merges_sorted ... ok'in text
(p/'test-status.json').write_text(json.dumps(dict(passed=459,failed=0,ignored=2,transfer_regression_passed=True,fts_sha256=hashlib.sha256(Path('src/fts.rs').read_bytes()).hexdigest()),indent=2)+'\n')
