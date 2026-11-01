from pathlib import Path
import tempfile,shutil,json,subprocess,hashlib
src=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-rollback-index-protocol-')as tmp:
 p=Path(tmp).resolve()
 for n in ['input.json','cases.json','baseline-store.rs','candidate-store.rs','compare.rs','analyze.py','verify.py','qualify.py','test-status.json','mutant-status.json','baseline-regression-status.json']:shutil.copy2(src/n,p/n)
 def write(n,d):(p/n).parent.mkdir(parents=True,exist_ok=True);(p/n).write_text(json.dumps(d)+'\n')
 def run(n,ok=True):
  r=subprocess.run(['python3',str(p/n)],capture_output=True,text=True);assert (r.returncode==0)==ok,(n,r.stdout,r.stderr)
 sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=json.loads((p/'input.json').read_text());cases=json.loads((p/'cases.json').read_text());binaries={}
 for role in ['baseline','candidate']:
  f=p/(role+'-binary');f.write_bytes(role.encode());binaries[role]=dict(path=str(f),sha256=sha(f),store_sha256=sha(p/(role+'-store.rs')))
 write('provenance.json',dict(source_restored=True,instrumentation=False,source_sha256=m['source_sha256'],original_compare_sha256=sha('benches/compare.rs'),compiled_compare_sha256=sha(p/'compare.rs'),binaries=binaries))
 smokes=[]
 for role in binaries:
  for case in cases:
   write('qualification-smoke/'+role+'-'+case.replace('/','_')+'/run.json',dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=100)]));smokes.append(dict(role=role,case=case,exit=0))
 write('qualification-smoke-status.json',smokes);records=[]
 for repeat in range(1,13):
  for index,case in enumerate(cases[repeat%14:]+cases[:repeat%14]):
   roles=['baseline','control','candidate'];shift=(repeat+index)%3
   for role in roles[shift:]+roles[:shift]:
    dest=p/f'{repeat:02}-{role}'/case.replace('/','_');binary=binaries['baseline'if role=='control'else role];value=75 if role=='candidate'and case in cases[:5]else 100
    write(str(dest.relative_to(p)/'run.json'),dict(status='complete',id=f'{repeat}-{role}-{case}',provenance=dict(samples='8',warmup_ns='0'),cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,value=value,availability=dict(state='available'))]*8))
    records.append(dict(round=repeat,mode=role,case=case,exit_code=0,binary_sha256=binary['sha256'],command=[binary['path'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]))
 write('process-status.json',records);run('analyze.py');run('verify.py');run('qualify.py')
 original=(p/'summary.json').read_bytes();d=json.loads(original);d[cases[0]]['series1']['gain_percent']=999;write('summary.json',d);run('verify.py',False);(p/'summary.json').write_bytes(original)
 write('process-status.json',records[:-1]);run('analyze.py',False);run('verify.py',False);write('process-status.json',records)
 records[0],records[1]=records[1],records[0];write('process-status.json',records);run('verify.py',False);records[0],records[1]=records[1],records[0];write('process-status.json',records)
 path=p/'01-candidate'/cases[0].replace('/','_')/'run.json';original=path.read_bytes();d=json.loads(original);d['observations'][0]['value']=float('nan');path.write_text(json.dumps(d));run('analyze.py',False);run('verify.py',False);path.write_bytes(original)
 d=json.loads(original);d['observations']=[dict(case=cases[0],metric='wall',operations=1,value=120,availability=dict(state='available'))]*8;path.write_text(json.dumps(d));run('analyze.py');run('verify.py');run('qualify.py',False);path.write_bytes(original)
 control=cases[5]
 for i in range(1,7):
  path=p/f'{i:02}-candidate'/control.replace('/','_')/'run.json';d=json.loads(path.read_text());d['observations']=[dict(case=control,metric='wall',operations=1,value=140,availability=dict(state='available'))]*8;path.write_text(json.dumps(d))
 run('analyze.py');run('verify.py');run('qualify.py',False)
 print('PASS: synthetic504-process matrix; summary/missing/order/NaN corruptions rejected; target loss and control regression retained and rejected by gate')
