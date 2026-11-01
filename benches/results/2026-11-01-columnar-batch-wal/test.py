from pathlib import Path
import subprocess,json,re,hashlib
p=Path(__file__).resolve().parent
with(p/'full-release.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
text=(p/'full-release.log').read_text();results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
assert r.returncode==0 and results
passed=sum(int(x[0])for x in results);ignored=sum(int(x[2])for x in results);assert passed==460 and ignored==2,(passed,ignored)
assert all(name+' ... ok' in text for name in ['column_batch_preserves_order_bits_and_existing_wire_replay','column_batch_rejects_incompatible_shapes_without_mutation'])
(p/'test-status.json').write_text(json.dumps(dict(passed=passed,ignored=ignored,persist_sha256=hashlib.sha256(Path('src/persist.rs').read_bytes()).hexdigest()))+'\n')
