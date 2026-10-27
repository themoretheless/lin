from pathlib import Path
import json,tempfile,hashlib,subprocess,sys,shutil
p=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-phase-protocol-')as tmp:
 t=Path(tmp);dump=lambda f,v:f.write_text(json.dumps(v)+'\n');cases=json.loads((p/'cases.json').read_text());dump(t/'cases.json',cases);roles=['original','control','disabled','traced'];names=['exec_program','quota','encode','crc','write','sync','checkpoint','rollback'];source=t/'source.rs';source.write_text('fixture');h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
 for f in ['profile.rs','original-compare.rs','instrumented-compare.rs','instrumented-exec.rs','instrumented-store.rs','instrumented-persist.rs','instrumented-lib.rs']:shutil.copy2(p/f,t/f)
 m=dict(source_restored=True,source_sha256={str(source):h(source)},original_compare_sha256=h('benches/compare.rs'),benchmark_sha256={role:h(t/f'{role}-compare.rs')for role in ['original','instrumented']},instrumented_source_sha256={n:h(t/f'instrumented-{n}.rs')for n in ['exec','store','persist','lib']},profile_sha256=h(t/'profile.rs'),binaries={role:dict(sha256=char*64)for role,char in [('original','a'),('instrumented','b')]});dump(t/'provenance.json',m)
 dump(t/'environment.json',dict(all_roles=roles,trace_enabled_roles=['traced'],allocator_unchanged_all_roles=True,allocator_environment=dict(GLIBC_TUNABLES=None)));dump(t/'test-status.json',dict(exit_code=0,profile='release',features='default'));records=[];status=[]
 for i in range(1,7):
  for role in roles:
   base=t/f'{i:02}-{role}';base.mkdir()
   for c in cases:
    name=c.replace('/','_');dest=base/name;dest.mkdir();value=(200 if c.endswith('/lin')else 400 if c.endswith('/sqlite_delete')else 100)+i+roles.index(role)*3
    dump(dest/'run.json',dict(status='complete',cases=[dict(id=c)],observations=[dict(case=c,metric='wall',operations=1,value=str(value+j))for j in range(8)]));(base/(name+'-resource.txt')).write_text('Maximum resident set size (kbytes): 2048\n')
    r=dict(round=i,mode=role,case=c,exit_code=0,peak_rss_kib=2048,binary_sha256=m['binaries']['original'if role in ['original','control']else 'instrumented']['sha256'],glibc_tunables=None,trace_enabled='1'if role=='traced'else '0')
    if role=='traced':
     frames=[]
     if c.endswith('/lin'):
      rollback='rollback_'in c
      for j in range(11):frames.append(dict(wall_ns=(200 if rollback else 1000)+j,phase_ns=[100,10,0 if rollback else 50,0 if rollback else 10,0 if rollback else 20,0 if rollback else 700,0,50 if rollback else 0],phase_calls=[1,1,int(not rollback),int(not rollback),int(not rollback),int(not rollback),0,int(rollback)],payload_bytes=0 if rollback else 200))
     trace=base/(name+'.phases.json');dump(trace,dict(phase_names=names,records=frames,scope='all invocations including validation/calibration',timing_qualified=False));r['trace_sha256']=h(trace)
    records.append(r)
   status.append(dict(run=f'{i:02}-{role}',exit_code=0,subprocesses=24))
 dump(t/'process-status.json',records);dump(t/'status.json',status)
 subprocess.run([sys.executable,str(p/'analyze.py'),str(t)],check=True);subprocess.run([sys.executable,str(p/'verify.py'),str(t)],check=True)
 resource=t/'01-original'/(cases[0].replace('/','_')+'-resource.txt');original=resource.read_text();resource.write_text(original.replace('2048','2049'))
 assert subprocess.run([sys.executable,str(p/'verify.py'),str(t)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0;resource.write_text(original)
 trace=t/'01-traced'/(cases[0].replace('/','_')+'.phases.json');original=trace.read_bytes();data=json.loads(original);data['records'][0]['phase_calls'][5]=0;dump(trace,data)
 for r in records:
  if r['round']==1 and r['mode']=='traced'and r['case']==cases[0]:r['trace_sha256']=h(trace)
 dump(t/'process-status.json',records)
 assert subprocess.run([sys.executable,str(p/'verify.py'),str(t)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
 print('PASS: complete synthetic matrix accepted; altered RSS and suppressed sync event rejected')
