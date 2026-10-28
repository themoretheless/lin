from pathlib import Path
import subprocess,json,os,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['all_sources_restored'];out=p/'local-smoke';out.mkdir(exist_ok=False);status=[]
for role in ['original','disabled','traced']:
 for size in ['1k','10k']:
  case=f'compare/insert_bulk_{size}/lin';b=m['binaries']['original'if role=='original'else'instrumented'];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256'];dest=out/f'{role}-{size}';trace=out/f'{role}-{size}.trace.json';env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C',LIN_BULK_TRACE='1'if role=='traced'else'0',LIN_BULK_TRACE_PATH=str(trace));env.pop('DYLD_INSERT_LIBRARIES',None);env.pop('LD_PRELOAD',None)
  cmd=[b['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  with dest.with_suffix('.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=env)
  assert r.returncode==0,(role,size)
  q=json.loads((dest/'run.json').read_text());assert q['status']=='complete'and len(q['observations'])==1 and q['cases'][0]['id']==case
  if role=='traced':
   t=json.loads(trace.read_text());assert len(t['phase_names'])==9 and t['timing_qualified']is False and len(t['records'])>0
   for record in t['records']:
    assert record['wall_ns']>0 and record['phase_calls']==[0,1,1]+[1000 if size=='1k'else 10000]*5+[1] and record['phase_ns'][0]==0 and sum(record['phase_ns'])<=record['wall_ns']
  else:assert not trace.exists()
  status.append(dict(role=role,case=case,exit=0,trace_verified=role=='traced',correctness_only=True));(p/'smoke-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert len(status)==6
print('six fresh full-value/phase-count smokes passed; no timing qualification')
