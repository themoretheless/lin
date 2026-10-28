from pathlib import Path
import tempfile,json,subprocess,ast,shutil,hashlib
p=Path(__file__).resolve().parent;cases=ast.literal_eval(next(s[6:]for s in(p/'run.py').read_text().splitlines()if s.startswith('cases=')))
with tempfile.TemporaryDirectory(prefix='lin-soa-synthetic-')as td:
 d=Path(td);shutil.copy2(p/'analyze.py',d/'analyze.py');(d/'provenance.json').write_text(json.dumps(dict(binaries={r:dict(sha256=r)for r in ['baseline','candidate']})));records=[]
 for round in range(1,13):
  for role in ['baseline','control','candidate']:
   for case in cases:
    target=case=='compare/insert_bulk_10k/lin';v=90 if target and role=='candidate'else 100
    q=dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=v)for _ in range(8)]);dest=d/f'{round:02}-{role}'/case.replace('/','_');dest.mkdir(parents=True);(dest/'run.json').write_text(json.dumps(q));resource=b'SYNTHETIC ONLY\nUser time (seconds): 0.1\nSystem time (seconds): 0.1\nMaximum resident set size (kbytes): 100\nExit status: 0\n';dest.with_suffix('.resources.txt').write_bytes(resource);records.append(dict(round=round,mode=role,case=case,exit_code=0,binary_sha256='baseline'if role=='control'else role,resource_sha256=hashlib.sha256(resource).hexdigest()))
 (d/'process-status.json').write_text(json.dumps(records))
 def run():return subprocess.run(['python3',str(d/'analyze.py')],capture_output=True)
 assert run().returncode==0;assert json.loads((d/'qualification.json').read_text())['component_gate_passes']
 resource=(d/'01-candidate'/cases[0].replace('/','_')).with_suffix('.resources.txt');saved_resource=resource.read_bytes();resource.unlink();assert run().returncode!=0;resource.write_bytes(saved_resource)
 f=d/'01-candidate'/cases[0].replace('/','_')/'run.json';original=f.read_bytes();f.unlink();assert run().returncode!=0;f.write_bytes(original)
 q=json.loads(original);q['observations'][0]['value']=-1;f.write_text(json.dumps(q));assert run().returncode!=0;f.write_bytes(original)
 q=json.loads(original)
 for o in q['observations']:o['value']=200
 f.write_text(json.dumps(q));assert run().returncode==0 and not json.loads((d/'qualification.json').read_text())['component_gate_passes']
print('SYNTHETIC ONLY: 324-process matrix passes; missing process, negative observation and losing target detected')
