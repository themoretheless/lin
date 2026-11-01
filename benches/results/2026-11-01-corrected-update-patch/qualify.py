from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text());status=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),performance_qualified=False)
try:
 for key in ['source_sha256','tests_sha256']:
  assert all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m[key].items())
 for name in ['test.py','check-mutants.py']:
  status[name]=subprocess.run(['python3',str(p/name)]).returncode
  assert status[name]==0,name
finally:
 status['sources_restored']=all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m['source_sha256'].items())
 status['tests_unchanged']=all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m['tests_sha256'].items())
 (p/'qualification-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['sources_restored']and status['tests_unchanged']
