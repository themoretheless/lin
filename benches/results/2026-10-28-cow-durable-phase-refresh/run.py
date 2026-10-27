from pathlib import Path
import subprocess,json,hashlib,os,time,platform,re
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
assert platform.system()=='Linux'and platform.machine()=='x86_64'
assert not os.environ.get('LD_PRELOAD')and not os.environ.get('LD_AUDIT')
assert not(p/'process-status.json').exists(),'No implicit rerun'
assert len(json.loads((p/'smoke-status.json').read_text()))==30
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),libc=platform.libc_ver(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),cpu=subprocess.check_output(['lscpu','-J'],text=True),allocator_environment={k:os.environ.get(k)for k in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','LD_PRELOAD','LD_AUDIT']},same_runtime_all_roles=True),indent=2)+'\n')
cases=[f'compare/durable_{op}_1row_{size}/lin'for size in ['1k','10k']for op in ['update','delete','commit','rollback']]+['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin']
records=[]
for repeat in range(1,7):
 ordered=cases[repeat%10:]+cases[:repeat%10]
 for case_index,case in enumerate(ordered):
  roles=['original','control','disabled','traced'];shift=(repeat+case_index)%4;roles=roles[shift:]+roles[:shift]
  for role in roles:
   binary=m['binaries']['original'if role in ['original','control']else'instrumented'];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
   base=p/f'{repeat:02}-{role}';base.mkdir(exist_ok=True);name=case.replace('/','_');output=base/name;trace=base/(name+'-trace.json');resource=base/(name+'-resource.txt')
   env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C',LIN_MUTATION_TRACE='1'if role=='traced'else'0',LIN_MUTATION_TRACE_PATH=str(trace));env.pop('LIN_BENCH_MIXED_CASE',None)
   cmd=['/usr/bin/time','-v','-o',str(resource),'--',binary['path'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(output)];started=time.time()
   with(base/(name+'.log')).open('w')as log:child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
   row=dict(round=repeat,mode=role,case=case,wrapper_pid=pid,exit_code=code,binary_sha256=binary['sha256'],elapsed_seconds=time.time()-started);records.append(row);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert code==0,row
   rss=re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',resource.read_text());assert rss and int(rss[1])>0;row['peak_rss_kib']=int(rss[1]);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n')
  print(repeat,case,'complete',flush=True)
assert len(records)==240
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==v for f,v in m['source_sha256'].items())
