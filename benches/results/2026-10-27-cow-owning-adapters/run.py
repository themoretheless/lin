from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;old=p.parent/'2026-10-27-cow-broad-engine';names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};target=Path('tests/cow_owning_adapters_probe.rs');implicit=Path('examples/cow_implicit_map_probe.rs');assert not target.exists()and not implicit.exists()
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},candidate_sha256={n:h((p if n=='store'else old)/f'candidate-{n}.rs')for n in names},tests_sha256=h(p/'owning-tests.rs'),performance_qualified=False,production_api_unchanged=True,roles={})
try:
 target.write_bytes((p/'owning-tests.rs').read_bytes());implicit.write_bytes((p/'implicit-map.rs').read_bytes())
 for role in ['baseline','candidate']:
  for n in names:Path(f'src/{n}.rs').write_bytes(before[n]if role=='baseline'else((p if n=='store'else old)/f'candidate-{n}.rs').read_bytes())
  with(p/f'{role}.log').open('w')as log:r=subprocess.run(['cargo','test','--test','cow_owning_adapters_probe','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  m['roles'][role]=dict(exit_code=r.returncode);assert r.returncode==0,(role,'see log')
  with(p/f'{role}-implicit.log').open('w')as log:check=subprocess.run(['cargo','check','--example','cow_implicit_map_probe','--color','never'],stdout=log,stderr=subprocess.STDOUT)
  m['roles'][role]['implicit_map_exit_code']=check.returncode
  assert (check.returncode==0)==(role=='baseline')
  if role=='candidate':assert 'E0308'in(p/f'{role}-implicit.log').read_text()

finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 target.unlink(missing_ok=True);implicit.unlink(missing_ok=True);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'status.json').write_text(json.dumps(m,indent=2)+'\n')
