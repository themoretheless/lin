from pathlib import Path
import json,tempfile,subprocess,shutil
p=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-normal-audit-')as tmp:
 t=Path(tmp);records=json.loads((p/'process-status.json').read_text())
 for name in ['probe.rs','provenance.json','process-status.json','summary.json','environment.json']+[r['output']for r in records]:shutil.copy2(p/name,t/name)
 def verify(ok):
  r=subprocess.run(['python3',str(p/'verify.py'),str(t)],capture_output=True,text=True);assert(r.returncode==0)==ok,r.stdout+r.stderr
 verify(True)
 f=t/'01-lin.json';original=f.read_text();data=json.loads(original);data['samples'][0]['checkpoint']=True;f.write_text(json.dumps(data));verify(False);f.write_text(original)
 data=json.loads(original);data['samples'].pop();f.write_text(json.dumps(data));verify(False);f.write_text(original)
 f=t/'summary.json';data=json.loads(f.read_text());data['runs'][0]['checkpoint_total_percent']+=1;f.write_text(json.dumps(data));verify(False)
 print('PASS: independent audit accepts originals and rejects altered checkpoint flag,missing observation and altered checkpoint share')
