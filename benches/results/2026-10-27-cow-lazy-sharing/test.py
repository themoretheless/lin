from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent;frozen=p.parent/'2026-10-27-cow-broad-engine';owning=p.parent/'2026-10-27-cow-owning-adapters';names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};target=Path('tests/cow_lazy_owning_probe.rs');assert not target.exists()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},candidate_sha256={n:h((p if n in ['exec','store']else frozen)/f'candidate-{n}.rs')for n in names},public_row_newtype_break=True,performance_qualified=False)
try:
 target.write_bytes((owning/'owning-tests.rs').read_bytes())
 for n in names:Path(f'src/{n}.rs').write_bytes(((p if n in ['exec','store']else frozen)/f'candidate-{n}.rs').read_bytes())
 for n,source in [('store',frozen/'oracle.rs'),('index',frozen/'index-oracle.rs'),('store',p/'oracle.rs')]:
  with Path(f'src/{n}.rs').open('a')as f:f.write('\n'+source.read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0,'inspect tests.log'
finally:
 target.unlink(missing_ok=True)
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n')
