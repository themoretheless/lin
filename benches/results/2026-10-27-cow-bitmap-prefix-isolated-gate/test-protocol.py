"""Synthetic harness sensitivity only; these are not performance results."""
from pathlib import Path
import ast,json,tempfile,subprocess,shutil
p=Path(__file__).resolve().parent
cases=next(ast.literal_eval(n.value)for n in ast.parse((p/'run.py').read_text()).body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='CASES'for t in n.targets))
assert len(cases)==20 and len(set(cases))==20
with tempfile.TemporaryDirectory(prefix='lin-isolated-protocol-')as folder:
 q=Path(folder);shutil.copy2(p/'summarize.py',q/'summarize.py');statuses=[];last=None
 for i in range(1,13):
  for mode,value in [('baseline',100),('control',102),('candidate',50)]:
   for case in cases:
    statuses.append(dict(round=i,mode=mode,case=case,exit_code=0));last=q/f'{i:02}-{mode}'/case.replace('/','_')/'run.json';last.parent.mkdir(parents=True)
    last.write_text(json.dumps(dict(status='complete',cases=[case],observations=[dict(case=case,metric='wall',operations=1,value=str(value))for _ in range(8)])))
 (q/'process-status.json').write_text(json.dumps(statuses));r=subprocess.run(['python3',str(q/'summarize.py')],capture_output=True,text=True);assert r.returncode==0,r.stderr
 s=json.loads((q/'summary.json').read_text());assert len(s)==20
 assert all(v['pairs']==6 and v['gain_percent']==50 and abs(v['aa_absolute_percent']-2)<1e-10 and v['positive_pairs']==6 for ss in s.values()for v in ss.values())
 raw=json.loads(last.read_text());raw['observations'][0]['operations']=2;last.write_text(json.dumps(raw));r=subprocess.run(['python3',str(q/'summarize.py')],capture_output=True,text=True);assert r.returncode!=0 and 'AssertionError'in r.stderr
 (q/'process-status.json').write_text(json.dumps(statuses[:-1]));r=subprocess.run(['python3',str(q/'summarize.py')],capture_output=True,text=True);assert r.returncode!=0 and 'AssertionError'in r.stderr
(p/'protocol-test.json').write_text(json.dumps({'scope':'synthetic harness sensitivity only','valid_720_process_fixture':True,'changed_operation_count_detected':True,'missing_process_detected':True},indent=2)+'\n')
print('PASS: synthetic protocol summary and two negative controls')
