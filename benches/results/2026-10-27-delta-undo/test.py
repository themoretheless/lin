from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['exec','store'];assert all(Path(f'src/{n}.rs').read_bytes()==(p/f'before-{n}.rs').read_bytes()for n in names);status={}
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 with Path('src/exec.rs').open('a')as f:f.write('\n'+(p/'oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--test','exec','--test','persist','--test','transaction_tests','--test','partial_index_mutation','--test','optimizer_tests','--test','semantics_tests','--test','query','--test','mutation_contract','--test','delete_readback','--test','hnsw_tests'],stdout=log,stderr=subprocess.STDOUT)
 status['exit_code']=r.returncode
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'before-{n}.rs').read_bytes())
 (p/'test-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['exit_code']==0
print('candidate/oracle tests passed; production source restored',flush=True)
