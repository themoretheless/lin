from pathlib import Path
import subprocess,json,hashlib,shutil,os
p=Path(__file__).resolve().parent;source=Path('src/exec.rs');original=source.read_bytes();example=Path('examples/transaction_schema_undo_probe.rs');assert not example.exists();example.parent.mkdir(exist_ok=True)
ref='21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5';h=lambda b:hashlib.sha256(b).hexdigest();status=dict(baseline_reference=ref,source_parent=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),performance_qualified=False)
try:
 source.write_bytes(subprocess.check_output(['git','show',ref+':src/exec.rs']));shutil.copyfile(p/'probe.rs',example)
 status['source_sha256']={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')}
 for n,d in status['source_sha256'].items():assert d==h(subprocess.check_output(['git','show',ref+':'+n])),n
 status['probe_sha256']=h(example.read_bytes());status['compiler']=subprocess.check_output(['rustc','-Vv'],text=True)
 with(p/'baseline-runtime.log').open('w')as log:r=subprocess.run(['cargo','run','--release','--example','transaction_schema_undo_probe','--color','never'],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-cow-vnext-target'),stdout=log,stderr=subprocess.STDOUT)
 status['runtime_exit']=r.returncode
 lines=(p/'baseline-runtime.log').read_text().splitlines();outcomes=[json.loads(s)for s in lines if s.startswith('{')];status['outcomes']=outcomes
 status['runtime_reproduced']=r.returncode==3 and len(outcomes)==6 and all(not o['passes']for o in outcomes)and 'Finished `release` profile'in'\n'.join(lines)
finally:
 source.write_bytes(original)
 if example.exists():example.unlink()
 status['source_restored']=source.read_bytes()==original
 (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['source_restored']and status['runtime_reproduced'],status
print('Reproduced six schema-after-row rollback failures on exact corrected baseline; source restored')
