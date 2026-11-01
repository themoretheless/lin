from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text());status=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),performance_qualified=False)
try:
 for key in ['source_sha256','tests_sha256']:
  assert all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m[key].items())
 for name in ['test.py','check-mutants.py']:
  status[name]=subprocess.run(['python3',str(p/name)]).returncode;assert status[name]==0,name
 with(p/'point-values-build.log').open('w')as log:r=subprocess.run(['cargo','bench','--bench','point_values','--no-run','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 paths=re.findall(r'Executable benches/point_values\.rs \(([^\n]+)\)',(p/'point-values-build.log').read_text());assert len(paths)==1
 with(p/'point-values-oracle.log').open('w')as log:r=subprocess.run([paths[0],'--verify-oracle'],stdout=log,stderr=subprocess.STDOUT)
 outcomes=[json.loads(s)for s in(p/'point-values-oracle.log').read_text().splitlines()if s.startswith('{')];assert r.returncode==0 and outcomes==[dict(full_value_checks=6,negative_oracle_checks=5,performance_qualified=False)]
 status['point_value_checks']=6;status['point_value_oracle_negatives']=5
finally:
 status['sources_restored']=all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m['source_sha256'].items())
 status['tests_unchanged']=all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m['tests_sha256'].items())
 (p/'qualification-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['sources_restored']and status['tests_unchanged']
