from pathlib import Path
import sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
import json,subprocess,tempfile,shutil
p=Path(__file__).resolve().parent;cases=json.loads((p/'benchmark-cases.json').read_text())
with tempfile.TemporaryDirectory(prefix='lin-fts-matrix-')as td:
 d=Path(td);root=d/'benchmark-measurement';root.mkdir();shutil.copy2(p/'benchmark-cases.json',d/'benchmark-cases.json')
 for name in ['benchmark-provenance.json']:(d/name).write_text(json.dumps(dict(binaries={r:dict(sha256=r)for r in ['baseline','candidate']})))
 rows=[]
 for repeat in range(1,13):
  for i,c in enumerate(cases[repeat%24:]+cases[:repeat%24]):
   original_index=next(j for j,x in enumerate(cases)if x['id']==c['id']);roles=[x['role']for x in schedule(cases,['baseline','control','candidate'])if x['round']==repeat and x['workload']==c['id']]
   for role in roles:
    name=f'{len(rows):04}';out=root/name;out.mkdir();v=80 if role=='candidate'else 100
    data=dict(id=name,status='complete',cases=[dict(id=c['id'])],observations=[dict(case=c['id'],metric='wall',operations=1,availability=dict(state='available'),value=v)for _ in range(8)])
    (out/'run.json').write_text(json.dumps(data));rows.append(dict(round=repeat,role=role,id=c['id'],exit_code=0,binary_sha256='baseline'if role=='control'else role,output=name))
 status=root/'process-status.json';status.write_text(json.dumps(rows))
 def run(ok):
  r=subprocess.run(['python3',str(p/'analyze.py'),str(d)],capture_output=True,text=True);assert (r.returncode==0)==ok,r.stderr[-2000:]
 run(True);assert json.loads((root/'qualification.json').read_text())['component_gate_passes']
 tests=[]
 status.write_text(json.dumps(rows[:-1]));run(False);tests.append('missing-process');status.write_text(json.dumps(rows))
 altered=list(rows);altered[0],altered[1]=altered[1],altered[0];status.write_text(json.dumps(altered));run(False);tests.append('process-order');status.write_text(json.dumps(rows))
 f=root/rows[0]['output']/'run.json';original=f.read_text();x=json.loads(original);x['observations'][0]['value']=float('nan');f.write_text(json.dumps(x));run(False);tests.append('nan');f.write_text(original)
 x=json.loads(original);x['observations'].pop();f.write_text(json.dumps(x));run(False);tests.append('missing-observation');f.write_text(original)
 target=next(r for r in rows if r['role']=='candidate'and next(c for c in cases if c['id']==r['id'])['target']);f=root/target['output']/'run.json';saved=f.read_text();x=json.loads(saved)
 for o in x['observations']:o['value']=101
 f.write_text(json.dumps(x));run(True);assert not json.loads((root/'qualification.json').read_text())['component_gate_passes'];tests.append('valid-losing-pair-rejected');f.write_text(saved)
 control=next(r for r in rows if r['role']=='candidate'and not next(c for c in cases if c['id']==r['id'])['target'])
 for r in rows:
  if r['role']=='candidate'and r['id']==control['id']:
   f=root/r['output']/'run.json';x=json.loads(f.read_text())
   for o in x['observations']:o['value']=110
   f.write_text(json.dumps(x))
 run(True);assert not json.loads((root/'qualification.json').read_text())['component_gate_passes'];tests.append('valid-regressing-control-rejected')
(p/'synthetic-status.json').write_text(json.dumps(dict(passed=True,checks=tests,processes=864,observations=6912,real_timings=False),indent=2)+'\n');print('Synthetic full matrix and six checks passed')
