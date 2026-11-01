from pathlib import Path
import tempfile,json,subprocess,shutil
p=Path(__file__).resolve().parent;cases=json.loads((p/'cases.json').read_text());roles=json.loads((p/'roles.json').read_text());checks=[]
with tempfile.TemporaryDirectory(prefix='lin-history-math-')as td:
 d=Path(td);root=d/'measurement';root.mkdir()
 for name in ['cases.json','roles.json']:shutil.copy2(p/name,d/name)
 records=[]
 for rnd in range(1,13):
  for i,c in enumerate(cases[rnd%8:]+cases[:rnd%8]):
   shift=(rnd+i)%5
   for r in roles[shift:]+roles[:shift]:
    name=f'{len(records):04}';out=root/name;out.mkdir();case=f"compare/seed_history_{r['mode']}_{c['operation']}_{c['size']}/{r['engine']}";v=80 if r['id']=='lin_grouped'else 100
    (out/'run.json').write_text(json.dumps(dict(id=name,status='complete',synthetic=True,cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,availability=dict(state='available'),value=v)for _ in range(8)])));records.append(dict(round=rnd,workload=c['id'],role=r['id'],case=case,exit_code=0,output=name))
 status=root/'process-status.json';status.write_text(json.dumps(records))
 def run(ok):
  r=subprocess.run(['python3',str(p/'analyze.py'),str(d)],capture_output=True,text=True);assert(r.returncode==0)==ok,r.stderr[-2000:]
 run(True);summary=json.loads((root/'summary.json').read_text());assert all(r['lin_group_gain_percent']==19.999999999999996 and r['lin_absolute_aa_percent']==0 and r['lin_positive_pairs']==6 and r['six_positive_above_aa']and r['chunked_lin_over_sqlite']==1 and r['grouped_lin_over_sqlite']==0.8 for series in summary.values()for r in series.values());checks.append('exact-20-percent-gain-and-0.8-ratio')
 status.write_text(json.dumps(records[:-1]));run(False);status.write_text(json.dumps(records));checks.append('missing-process-rejected')
 x=list(records);x[0],x[1]=x[1],x[0];status.write_text(json.dumps(x));run(False);status.write_text(json.dumps(records));checks.append('wrong-order-rejected')
 f=root/records[0]['output']/'run.json';original=f.read_text();x=json.loads(original);x['observations'][0]['value']=float('nan');f.write_text(json.dumps(x));run(False);f.write_text(original);checks.append('nan-rejected')
 x=json.loads(original);x['observations'].pop();f.write_text(json.dumps(x));run(False);f.write_text(original);checks.append('missing-observation-rejected')
 for row in records:
  if row['role']=='lin_grouped':
   f=root/row['output']/'run.json';x=json.loads(f.read_text())
   for o in x['observations']:o['value']=110
   f.write_text(json.dumps(x))
 run(True);summary=json.loads((root/'summary.json').read_text());assert all(r['lin_group_gain_percent']<0 and r['lin_positive_pairs']==0 and not r['six_positive_above_aa']for series in summary.values()for r in series.values());assert json.loads((root/'diagnostic-status.json').read_text())['performance_qualification']is False;checks.append('valid-regression-retained-without-adoption')
(p/'test-analysis-status.json').write_text(json.dumps(dict(passed=True,synthetic=True,real_timings=False,processes=480,observations=3840,checks=checks),indent=2)+'\n');print('PASS history matrix math and five failure/regression checks')
