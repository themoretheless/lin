"""Synthetic verifier controls only, never benchmark performance evidence."""
from pathlib import Path
import ast,importlib.util,json,tempfile,shutil,subprocess
root=Path(__file__).resolve().parents[1];proto=root/'benches/results/2026-10-27-cow-case-isolated-gate'
spec=importlib.util.spec_from_file_location('audit',root/'scripts/verify-case-isolated-mutation.py');audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
cases=next(ast.literal_eval(n.value)for n in ast.parse((proto/'run.py').read_text()).body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='CASES'for t in n.targets))
def rejected(p):
    try:audit.verify(p)
    except AssertionError:return
    raise AssertionError('malformed fixture accepted')
with tempfile.TemporaryDirectory(prefix='lin-isolated-audit-')as folder:
    p=Path(folder);shutil.copy2(proto/'run.py',p/'run.py');shutil.copy2(proto/'summarize.py',p/'summarize.py');records=[];groups=[];last=None
    for i in range(1,13):
        for mode,value in [('baseline',100),('control',102),('candidate',50)]:
            groups.append(dict(run=f'{i:02}-{mode}',exit_code=0,subprocesses=20))
            for case in cases:
                records.append(dict(round=i,mode=mode,case=case,exit_code=0));last=p/f'{i:02}-{mode}'/case.replace('/','_')/'run.json';last.parent.mkdir(parents=True)
                last.write_text(json.dumps(dict(status='complete',cases=[case],observations=[dict(case=case,metric='wall',operations=1,value=str(value))for _ in range(8)])))
    (p/'process-status.json').write_text(json.dumps(records));(p/'status.json').write_text(json.dumps(groups));(p/'qualification.json').write_text(json.dumps(dict(candidate_qualified=True,complete_rounds=12,full_objective_achieved=False)))
    subprocess.run(['python3',str(p/'summarize.py')],check=True,capture_output=True)
    result=audit.verify(p);assert result['candidate_qualified']and result['candidate_sqlite_wins']==0
    summary=(p/'summary.json').read_bytes();s=json.loads(summary);s[cases[0]]['series1']['gain_percent']=51;(p/'summary.json').write_text(json.dumps(s));rejected(p);(p/'summary.json').write_bytes(summary)
    raw=last.read_bytes();d=json.loads(raw);d['observations'].pop();last.write_text(json.dumps(d));rejected(p);last.write_bytes(raw)
    (p/'process-status.json').write_text(json.dumps(records[:-1]));rejected(p)
print('PASS: synthetic complete fixture; altered median, missing sample and missing process rejected')
