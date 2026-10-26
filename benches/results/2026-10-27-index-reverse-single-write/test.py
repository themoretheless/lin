from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes();assert before==(p/'before-index.rs').read_bytes()
try:
 f.write_bytes((p/'candidate-index.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--test','partial_index_mutation','--test','transaction_tests','--test','persist','--test','exec','--test','query','--test','semantics_tests','--test','mutation_contract'],stdout=log,stderr=subprocess.STDOUT)
 (p/'status.json').write_text(json.dumps({'exit_code':r.returncode},indent=2)+'\n')
 assert r.returncode==0
finally:f.write_bytes(before)
print('Tests passed; source restored')
