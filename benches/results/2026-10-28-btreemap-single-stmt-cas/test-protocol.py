from pathlib import Path
import tempfile,json,subprocess,shutil
src=Path(__file__).resolve().parent
cases=['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin','compare/update_1row_1k/lin','compare/update_1row_100k/lin'];checks=[]
with tempfile.TemporaryDirectory(prefix='lin-btreemap-cas-synthetic-')as td:
 p=Path(td)
 for f in src.iterdir():
  if f.is_file()and f.suffix not in ['.gz']:shutil.copy2(f,p/f.name)
 def write(name,value):(p/name).write_text(json.dumps(value)+'\n')
 def run(name,success=True):
  r=subprocess.run(['python3',str(p/name)],capture_output=True,text=True);assert (r.returncode==0)==success,(name,r.stdout,r.stderr)
 records=[]
 for repeat in range(1,13):
  for role in ['baseline','control','candidate']:
   for case in cases:
    dest=p/f'{repeat:02}-{role}'/case.replace('/','_');dest.mkdir(parents=True)
    value=80 if role=='candidate'and 'wal_scalar_'in case else 100
    write(str(dest.relative_to(p)/'run.json'),dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=str(value))for _ in range(8)]))
    records.append(dict(round=repeat,mode=role,case=case,exit_code=0,binary_sha256='c'if role=='candidate'else'b'))
 provenance=json.loads((p/'provenance.json').read_text());provenance['binaries']['baseline']['sha256']='b';provenance['binaries']['candidate']['sha256']='c';write('provenance.json',provenance);write('environment.json',dict(platform='synthetic Linux x86_64'));write('process-status.json',records)
 run('analyze.py');run('audit.py');q=json.loads((p/'qualification.json').read_text());assert q['component_gate_passes']and q['processes']==144 and q['observations']==1152
 checks.append(dict(check='complete-synthetic-matrix',passes=True))
 original=(p/'summary.json').read_text();summary=json.loads(original);summary[cases[0]]['series1']['passes']=False;write('summary.json',summary);run('audit.py',False);(p/'summary.json').write_text(original);checks.append(dict(check='changed-summary',rejected=True))
 write('process-status.json',records[:-1]);run('analyze.py',False);run('audit.py',False);write('process-status.json',records);checks.append(dict(check='missing-process',rejected=True))
 f=p/'01-candidate'/cases[0].replace('/','_')/'run.json';original=f.read_text();data=json.loads(original);data['observations'][0]['value']='NaN';f.write_text(json.dumps(data));run('analyze.py',False);run('audit.py',False);f.write_text(original);checks.append(dict(check='nan-wall',rejected=True))
 data=json.loads(original);data['observations']=[dict(case=cases[0],metric='wall',operations=1,value=120)]*8;f.write_text(json.dumps(data));run('analyze.py');run('audit.py');assert not json.loads((p/'qualification.json').read_text())['component_gate_passes'];f.write_text(original);checks.append(dict(check='losing-target-pair',gate_rejected=True))
 control=cases[2]
 for repeat in range(1,7):
  f=p/f'{repeat:02}-candidate'/control.replace('/','_')/'run.json';data=json.loads(f.read_text());data['observations']=[dict(case=control,metric='wall',operations=1,value=140)]*8;f.write_text(json.dumps(data))
 run('analyze.py');run('audit.py');assert not json.loads((p/'qualification.json').read_text())['component_gate_passes'];checks.append(dict(check='regressing-control',gate_rejected=True))
(src/'synthetic-validation.json').write_text(json.dumps(dict(synthetic_only=True,actual_measurement_complete=False,checks=checks),indent=2)+'\n');print('BTreeMap synthetic matrix and corruption/losing-target/control checks passed')
