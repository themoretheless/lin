from pathlib import Path
import subprocess,json,os,hashlib,time,platform,sys
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['all_sources_restored'];assert sys.platform=='linux'and platform.machine()=='x86_64';assert not(p/'process-status.json').exists(),'Preserve existing observations'
assert not os.environ.get('LD_PRELOAD')and not os.environ.get('DYLD_INSERT_LIBRARIES');(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False,observer_calibration=True),indent=2)+'\n');records=[]
cases=['compare/insert_bulk_1k/lin','compare/insert_bulk_10k/lin'];roles=['original','control','disabled','traced']
for round in range(1,7):
 order=cases[round%2:]+cases[:round%2]
 for i,case in enumerate(order):
  shift=(round+i)%4
  for role in roles[shift:]+roles[:shift]:
   binary=m['binaries']['original'if role in ['original','control']else'instrumented'];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256'];dest=p/f'{round:02}-{role}'/case.replace('/','_');dest.parent.mkdir(exist_ok=True);trace=dest.with_suffix('.trace.json');env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C',LIN_BULK_TRACE='1'if role=='traced'else'0',LIN_BULK_TRACE_PATH=str(trace));cmd=[binary['path'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   start=time.monotonic()
   with dest.with_suffix('.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=env)
   record=dict(round=round,role=role,case=case,exit=r.returncode,elapsed_seconds=time.monotonic()-start,binary_sha256=binary['sha256'],trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest()if trace.exists()else None);records.append(record);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0
  print(round,case,'complete',flush=True)
assert len(records)==48
