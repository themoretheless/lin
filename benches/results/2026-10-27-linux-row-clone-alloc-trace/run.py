from pathlib import Path
import subprocess,json,hashlib,os,time,platform
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary=Path('/tmp/lin-row-clone-alloc-trace');assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];status=[]
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),cpu_count=os.cpu_count(),cpu_info=Path('/proc/cpuinfo').read_text().split('\n\n')[0]if Path('/proc/cpuinfo').exists()else None,rustc=subprocess.check_output(['rustc','-Vv'],text=True),allocator_environment={key:os.environ.get(key)for key in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','MALLOC_MMAP_THRESHOLD_','MALLOC_TRIM_THRESHOLD_']}),indent=2)+'\n')
for i in range(1,7):
 roles=['baseline','control','trace'];shift=i%3;roles=roles[shift:]+roles[:shift]
 for role in roles:
  name=f'{i:02}-{role}';env=dict(os.environ,LIN_TRACE_ROW_ALLOCATIONS='1'if role=='trace'else'0',LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None);load=os.getloadavg();started=time.time()
  cmd=[str(binary),'--filter','compare/update_1row_*/lin','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  with(p/f'{name}.log').open('w')as log:child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
  status.append(dict(run=name,role=role,pid=pid,started_unix=started,exit_code=code,elapsed_seconds=time.time()-started,load_before=load,load_after=os.getloadavg()));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert code==0
  data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24 and all(o['operations']==1 for o in data['observations'])
  rows=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert len(rows)==33 and all(r['kind']=='update'and len(r['stages_ns'])==25 for r in rows);assert all(sum(r['rows']==n for r in rows)==11 for n in [1000,10000,100000])
  for r in rows:
   trace=r['allocation_trace'];assert trace['enabled']==(role=='trace')and not trace['overflow']and trace['count']==len(trace['events'])<=64
   if role=='trace':assert trace['count']>trace['fields']and all(e[5]==0 for e in trace['events'])and sum(e[4]for e in trace['events'])<=r['stages_ns'][21]
   else:assert trace['count']==0
  (p/f'{name}-phases.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,'complete',flush=True)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
