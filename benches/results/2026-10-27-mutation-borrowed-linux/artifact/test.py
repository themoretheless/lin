from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;names=['exec','store'];status={}
assert all(Path(f'src/{n}.rs').read_bytes()==(p/f'before-{n}.rs').read_bytes()for n in names)
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 with (p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--test','exec','--test','persist','--test','transaction_tests','--test','delete_readback','--test','hnsw_tests'],stdout=log,stderr=subprocess.STDOUT)
 status['focused_tests_exit']=r.returncode
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'before-{n}.rs').read_bytes())
 (p/'test-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['focused_tests_exit']==0
print('candidate tests passed; production source restored',flush=True)
