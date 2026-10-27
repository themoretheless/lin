from pathlib import Path
import subprocess,json,hashlib,os,time,platform,re
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary=Path('/tmp/lin-tcache-intervention');assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];assert platform.system()=='Linux'and platform.libc_ver()==('glibc','2.39');assert Path('/usr/bin/time').is_file();status=[]
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),libc=platform.libc_ver(),cpu_count=os.cpu_count(),cpu_info=Path('/proc/cpuinfo').read_text().split('\n\n')[0],rustc=subprocess.check_output(['rustc','-Vv'],text=True),gnu_time=subprocess.check_output(['/usr/bin/time','--version'],text=True),allocator_environment={key:os.environ.get(key)for key in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','MALLOC_MMAP_THRESHOLD_','MALLOC_TRIM_THRESHOLD_']}),indent=2)+'\n')
def replace_tunable(original,key,value):
 components=[s for s in(original or'').split(':')if s and s.split('=',1)[0]!=key];return ':'.join(components+[key+'='+str(value)])
for i in range(1,7):
 roles=['baseline','control','count32','unsorted16'];shift=i%4;roles=roles[shift:]+roles[:shift]
 for role in roles:
  name=f'{i:02}-{role}';env=dict(os.environ,LIN_TRACE_ROW_ALLOCATIONS='1',LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None)
  if role=='count32':env['GLIBC_TUNABLES']=replace_tunable(env.get('GLIBC_TUNABLES'),'glibc.malloc.tcache_count',32)
  if role=='unsorted16':env['GLIBC_TUNABLES']=replace_tunable(env.get('GLIBC_TUNABLES'),'glibc.malloc.tcache_unsorted_limit',16)
  load=os.getloadavg();started=time.time();cmd=['/usr/bin/time','-v','-o',str(p/f'{name}-resource.txt'),'--',str(binary),'--filter','compare/update_1row_*/lin','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  with(p/f'{name}.log').open('w')as log:child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);wrapper_pid=child.pid;code=child.wait()
  row=dict(run=name,role=role,wrapper_pid=wrapper_pid,started_unix=started,exit_code=code,elapsed_seconds=time.time()-started,glibc_tunables=env.get('GLIBC_TUNABLES'),load_before=load,load_after=os.getloadavg());status.append(row);(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert code==0
  rss=re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(p/f'{name}-resource.txt').read_text());assert rss and int(rss[1])>0;row['peak_rss_kib']=int(rss[1])
  data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24 and all(o['operations']==1 for o in data['observations'])
  rows=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert len(rows)==33 and all(r['kind']=='update'and len(r['stages_ns'])==25 for r in rows);assert all(sum(r['rows']==n for r in rows)==11 for n in [1000,10000,100000])
  pids={r['allocation_trace']['pid']for r in rows};assert len(pids)==1;row['benchmark_pid']=pids.pop();assert row['benchmark_pid']!=wrapper_pid
  for r in rows:
   trace=r['allocation_trace'];assert trace['enabled']and not trace['overflow']and trace['count']==len(trace['events'])<=64 and trace['count']>trace['fields']and all(e[5]==0 for e in trace['events'])and sum(e[4]for e in trace['events'])<=r['stages_ns'][21]
  (p/'status.json').write_text(json.dumps(status,indent=2)+'\n');(p/f'{name}-phases.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,'complete',flush=True)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
