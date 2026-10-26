from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['exec','store'];target=Path('tests/mutation_undo_contract.rs');assert not target.exists();assert all(Path(f'src/{n}.rs').read_bytes()==(p/f'before-{n}.rs').read_bytes()for n in names);status={}
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 target.write_bytes((p/'mutation_contract.rs').read_bytes())
 with (p/'contracts.log').open('w')as log:r=subprocess.run(['cargo','test','--test','mutation_undo_contract'],stdout=log,stderr=subprocess.STDOUT)
 status['exit_code']=r.returncode
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'before-{n}.rs').read_bytes())
 target.unlink(missing_ok=True);(p/'contract-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['exit_code']==0
print('journal and affected-count contracts passed; source restored',flush=True)
