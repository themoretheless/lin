from pathlib import Path
import json,hashlib,subprocess,tempfile,shutil
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();cases=json.loads((p/'benchmark-cases.json').read_text())
with tempfile.TemporaryDirectory(prefix='lin-column-batch-verifier-')as td:
 d=Path(td)
 for name in ['frozen-benchmark.json','full-release.log','candidate-persist.rs','lost-child-mutant.rs','lost-child-runtime.log','mutant-status.json','benchmark-compare.rs','legacy-provenance.json','legacy-results/status.json','legacy-oracle-negative/status.json','functional/status.json','benchmark-cases.json']:
  f=d/name;f.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p/name,f)
 m=json.loads((p/'benchmark-provenance.json').read_text());m['other_source_sha256']={str(f):sha(f.read_bytes())for f in Path('src').rglob('*.rs')if str(f)!='src/persist.rs'};(d/'benchmark-provenance.json').write_text(json.dumps(m))
 for folder,smoke in [('benchmark-smoke',True),('benchmark-measurement',False)]:
  root=d/folder;root.mkdir();rows=[]
  for rnd in range(1,2 if smoke else 13):
   for i,c in enumerate(cases[rnd%24:]+cases[:rnd%24]):
    roles=['baseline','candidate']if smoke else['baseline','control','candidate'];shift=(rnd+i)%len(roles)
    for role in roles[shift:]+roles[:shift]:
     name=f'{len(rows):04}';out=root/name;out.mkdir();b=m['binaries']['baseline'if role=='control'else role];samples=1 if smoke else 8;cmd=[b['path'],'--filter',c['id'],'--samples',str(samples),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
     data=dict(id=folder+name,status='complete',synthetic=True,cases=[dict(id=c['id'])],observations=[dict(case=c['id'],metric='wall',operations=1,availability=dict(state='available'),value=80 if role=='candidate'else 100)for _ in range(samples)])
     (out/'run.json').write_text(json.dumps(data));rows.append(dict(round=rnd,role=role,id=c['id'],command=cmd,exit_code=0,binary_sha256=b['sha256'],output=name))
  (root/'process-status.json').write_text(json.dumps(rows));(root/'status.json').write_text(json.dumps(dict(complete=True)))
 subprocess.run(['python3',str(p/'analyze.py'),str(d)],check=True,capture_output=True)
 # Execute verifier against a relocated synthetic root, retaining its real code.
 code=(p/'verify-benchmark.py').read_text().replace("p=Path(__file__).resolve().parent;",f'p=Path({str(d)!r});')
 def run(ok):
  r=subprocess.run(['python3','-c',code],capture_output=True,text=True);assert (r.returncode==0)==ok,r.stderr[-2000:]
 run(True)
 status=d/'benchmark-measurement/process-status.json';original=status.read_text();rows=json.loads(original);rows[0]['command'][2]='wrong/case';status.write_text(json.dumps(rows));run(False);status.write_text(original)
 rows=json.loads(original);rows[0],rows[1]=rows[1],rows[0];status.write_text(json.dumps(rows));run(False);status.write_text(original)
 summary=d/'benchmark-measurement/summary.json';s=json.loads(summary.read_text());s[cases[0]['id']]['series1']['pairs'][0]['candidate_ns']=1;summary.write_text(json.dumps(s));run(False)
(p/'test-verifier-status.json').write_text(json.dumps(dict(passed=True,synthetic=True,real_timings=False,checks=['complete-matrix-accepted','wrong-command-rejected','wrong-order-rejected','tampered-pair-rejected']),indent=2)+'\n');print('PASS independent verifier synthetic matrix and three corruption checks')
