from pathlib import Path
import tempfile,shutil,json,subprocess
p=Path(__file__).resolve().parent;records=json.loads((p/'process-status.json').read_text())
with tempfile.TemporaryDirectory(prefix='lin-checkpoint-stage-audit-')as tmp:
 t=Path(tmp)
 for name in ['probe.rs','instrumented-store.rs','provenance.json','process-status.json','summary.json','environment.json']+[r['output']for r in records]+[r['trace']for r in records]:shutil.copy2(p/name,t/name)
 def verify(ok):
  r=subprocess.run(['python3',str(p/'verify.py'),str(t)],capture_output=True,text=True);assert(r.returncode==0)==ok,r.stdout+r.stderr
 verify(True)
 f=t/'01-traced.log';original=f.read_text();lines=original.splitlines();assert lines[0].startswith('LIN_CHECKPOINT_STAGE ')
 data=json.loads(lines[0].partition(' ')[2]);data['stage_ns'][3]=-1;lines[0]='LIN_CHECKPOINT_STAGE '+json.dumps(data);f.write_text('\n'.join(lines)+'\n');verify(False)
 f.write_text('\n'.join(original.splitlines()[1:])+'\n');verify(False);f.write_text(original)
 f=t/'summary.json';d=json.loads(f.read_text());d['observer'][0]['traced_over_disabled']+=0.01;f.write_text(json.dumps(d));verify(False)
 print('PASS: original stage data accepted; negative phase,missing automatic checkpoint,and altered observer ratio rejected')
