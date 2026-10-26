from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/exec.rs');before=f.read_bytes();assert before==(p/'before-exec.rs').read_bytes()
try:
 f.write_bytes((p/'candidate-exec.rs').read_bytes()+b'\n'+(p/'oracle.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--test','exec','--test','persist','--test','transaction_tests','--test','partial_index_mutation','--test','semantics_tests','--test','query','--test','mutation_contract','--test','delete_readback','--test','snapshot_tests','--test','security_tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'status.json').write_text(json.dumps({'exit_code':r.returncode},indent=2)+'\n');assert r.returncode==0
finally:f.write_bytes(before)
print('Compact undo tests passed; production source restored')
