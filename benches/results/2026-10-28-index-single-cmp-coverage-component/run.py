from pathlib import Path
import subprocess,json,hashlib,os,time,platform
p=Path(__file__).resolve().parent
m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
cases=['compare/filter_eq/lin', 'compare/filter_range/lin', 'compare/point_get/lin', 'compare/text_substr/lin', 'compare/update_1row_1k/lin', 'compare/wal_scalar_full_update/lin', 'compare/filter_eq/sqlite', 'compare/filter_eq/duckdb']
assert not (p/'process-status.json').exists(),'Preserve prior observations; no implicit rerun'
assert not os.environ.get('DYLD_INSERT_LIBRARIES') and not os.environ.get('LD_PRELOAD')
env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None)
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),same_runtime_all_roles=True,memory_measured=False),indent=2)+'\n')
records=[]
for repeat in range(1,13):
 ordered=cases[repeat%len(cases):]+cases[:repeat%len(cases)]
 for case_index,case in enumerate(ordered):
  roles=['baseline','control','candidate'];shift=(repeat+case_index)%3;roles=roles[shift:]+roles[:shift]
  for role in roles:
   binary=m['binaries']['baseline' if role=='control' else role];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
   dest=p/f'{repeat:02}-{role}'/case.replace('/','_');dest.parent.mkdir(exist_ok=True)
   cmd=[binary['path'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   start=time.time()
   with dest.with_suffix('.log').open('w') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
   records.append(dict(round=repeat,mode=role,case=case,exit_code=r.returncode,binary_sha256=binary['sha256'],elapsed_seconds=time.time()-start));(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0,records[-1]
  print(repeat,case,'complete',flush=True)
assert len(records)==288
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==v for f,v in m['source_sha256'].items())
