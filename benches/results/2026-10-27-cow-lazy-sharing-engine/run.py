from pathlib import Path
import subprocess,json,hashlib,os,time,re,math
from runtime import capture,env_for
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());capture(p);CASES=json.loads((p/'cases.json').read_text());assert len(CASES)==len(set(CASES))==37;processes=[]
for round in range(1,13):
 cases=CASES[round%37:]+CASES[:round%37]
 for case_index,case in enumerate(cases):
  modes=['baseline','control','candidate'];shift=(round+case_index)%3;modes=modes[shift:]+modes[:shift]
  for mode in modes:
   binary=Path(meta['binaries']['baseline'if mode=='control'else mode]['path']);assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binaries']['baseline'if mode=='control'else mode]['sha256']
   group=f'{round:02}-{mode}';name=case.replace('/','_');dest=p/group/name;(p/group).mkdir(exist_ok=True);env=env_for(mode);load=os.getloadavg();started=time.time();resource=p/group/(name+'-resource.txt')
   cmd=['/usr/bin/time','-v','-o',str(resource),'--',str(binary),'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   with(p/group/(name+'.log')).open('w')as log:child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);wrapper_pid=child.pid;code=child.wait()
   row=dict(round=round,mode=mode,case=case,wrapper_pid=wrapper_pid,exit_code=code,started_unix=started,elapsed_seconds=time.time()-started,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),glibc_tunables=env.get('GLIBC_TUNABLES'),load_before=load,load_after=os.getloadavg());processes.append(row);(p/'process-status.json').write_text(json.dumps(processes,indent=2)+'\n');assert code==0
   match=re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',resource.read_text());assert match and int(match[1])>0;row['peak_rss_kib']=int(match[1]);(p/'process-status.json').write_text(json.dumps(processes,indent=2)+'\n')
   data=json.loads((dest/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==1 and len(data['observations'])==8
   assert all(o['case']==case and o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in data['observations'])
  print(round,case,'complete',flush=True)
 status=[]
 for i in range(1,round+1):
  for mode in ['baseline','control','candidate']:
   group=[r for r in processes if r['round']==i and r['mode']==mode];assert len(group)==37;status.append(dict(run=f'{i:02}-{mode}',exit_code=0,subprocesses=37))
 (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
assert len(processes)==1332;assert all(hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']for b in meta['binaries'].values());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
