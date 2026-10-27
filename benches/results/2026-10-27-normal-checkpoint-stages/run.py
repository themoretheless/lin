from pathlib import Path
import os,json,subprocess,hashlib,tempfile,time,platform
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items());records=[]
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),machine=platform.machine(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),diagnostic_only=True,policy_changed=False,clock_observer_controls=True),indent=2)+'\n')
for repeat in range(1,4):
 roles=['original','control','disabled','traced','sqlite'];shift=repeat%5;roles=roles[shift:]+roles[:shift]
 for role in roles:
  instrumented=role in ['disabled','traced'];binary=m['binaries']['instrumented'if instrumented else'original'];assert h(binary['path'])==binary['sha256'];engine='sqlite'if role=='sqlite'else'lin';env=dict(os.environ);env.pop('LIN_BENCH_CHECKPOINT_STAGES',None)
  if role=='traced':env['LIN_BENCH_CHECKPOINT_STAGES']='1'
  with tempfile.TemporaryDirectory(prefix='lin-checkpoint-stage-')as tmp:
   stdout=p/f'{repeat:02}-{role}.json';stderr=p/f'{repeat:02}-{role}.log';started=time.time()
   with stdout.open('w')as out,stderr.open('w')as err:
    child=subprocess.Popen([binary['path'],engine,str(Path(tmp)/engine)],stdout=out,stderr=err,env=env);pid=child.pid;code=child.wait()
   record=dict(repeat=repeat,role=role,engine=engine,pid=pid,exit_code=code,binary_sha256=binary['sha256'],elapsed_seconds=time.time()-started,output=stdout.name,trace=stderr.name,trace_enabled=role=='traced');records.append(record);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert code==0,(repeat,role)
 assert all(h(f)==v for f,v in m['source_sha256'].items())
assert len(records)==15;print('PASS:15 fresh processes /15000 transactions,full input-value reopen; baseline/control and disabled/traced retained')
