from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/index.rs');before=source.read_bytes();status=[]
try:
 for role in ['baseline','candidate']:
  source.write_bytes((p/f'{role}-index.rs').read_bytes()+b'\n'+(p/'oracle.rs').read_bytes())
  cmd=['cargo','test','--lib','exact_cardinality_stateful_model'] if role=='baseline'else ['cargo','test','--lib','--test','exec','--test','partial_index_mutation','--test','prepared_grouped_count','--test','transaction_tests']
  with(p/f'tests-{role}.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(role=role,exit_code=r.returncode,command=cmd));assert r.returncode==0
finally:
 source.write_bytes(before);(p/'test-status.json').write_text(json.dumps(dict(runs=status,source_restored=source.read_bytes()==before),indent=2)+'\n')
