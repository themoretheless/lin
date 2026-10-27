from pathlib import Path
import subprocess,json,hashlib,os,time,re,math
from runtime import capture,env_for,ROLES
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());capture(p);cases=json.loads((p/'cases.json').read_text());assert len(cases)==len(set(cases))==24;records=[]
for i in range(1,7):
 ordered=cases[i%24:]+cases[:i%24]
 for index,case in enumerate(ordered):
  shift=(i+index)%4;roles=ROLES[shift:]+ROLES[:shift]
  for role in roles:
   b=m['binaries']['original'if role in ['original','control']else 'instrumented'];binary=Path(b['path']);assert hashlib.sha256(binary.read_bytes()).hexdigest()==b['sha256']
   base=p/f'{i:02}-{role}';base.mkdir(exist_ok=True);name=case.replace('/','_');dest=base/name;resource=base/(name+'-resource.txt');trace=base/(name+'.phases.json');assert not dest.exists()and not trace.exists()
   env=env_for(role,trace);started=time.time();load=os.getloadavg()
   cmd=['/usr/bin/time','-v','-o',str(resource),'--',str(binary),'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   with(base/(name+'.log')).open('w')as log:child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
   r=dict(round=i,mode=role,case=case,wrapper_pid=pid,exit_code=code,started_unix=started,elapsed_seconds=time.time()-started,binary_sha256=b['sha256'],glibc_tunables=env.get('GLIBC_TUNABLES'),trace_enabled=env['LIN_MUTATION_TRACE'],load_before=load,load_after=os.getloadavg());records.append(r);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert code==0
   r['peak_rss_kib']=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',resource.read_text())[1]);assert r['peak_rss_kib']>0
   if role=='traced':assert trace.is_file();r['trace_sha256']=hashlib.sha256(trace.read_bytes()).hexdigest()
   else:assert not trace.exists()
   (p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n')
   d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==8 and len(d['cases'])==1
   assert all(o['case']==case and o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in d['observations'])
  print(i,case,'complete',flush=True)
 status=[]
 for j in range(1,i+1):
  for role in ROLES:
   group=[r for r in records if r['round']==j and r['mode']==role];assert len(group)==24;status.append(dict(run=f'{j:02}-{role}',exit_code=0,subprocesses=24))
 (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
assert len(records)==576 and all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
assert all(hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']for b in m['binaries'].values())
