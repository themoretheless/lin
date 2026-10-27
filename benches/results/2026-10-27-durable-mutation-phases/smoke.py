from pathlib import Path
import json,subprocess,os,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());cases=json.loads((p.parent/'2026-10-27-durable-mutation-baseline'/'cases.json').read_text());status=[]
for role in ['original','disabled','traced']:
 b=m['binaries']['original'if role=='original'else 'instrumented'];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
 for case in cases:
  dest=p/'smoke'/role/case.replace('/','_');dest.parent.mkdir(parents=True,exist_ok=True);trace=dest.with_suffix('.phases.json');env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1',LIN_MUTATION_TRACE='1'if role=='traced'else '0',LIN_MUTATION_TRACE_PATH=str(trace));env.pop('LIN_BENCH_MIXED_CASE',None)
  with dest.with_suffix('.log').open('w')as log:r=subprocess.run([b['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)],stdout=log,stderr=subprocess.STDOUT,env=env)
  assert r.returncode==0,(role,case)
  d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==1 and d['observations'][0]['operations']==1 and d['observations'][0]['case']==case
  n=0
  if role=='traced':
   t=json.loads(trace.read_text());assert len(t['phase_names'])==8 and t['timing_qualified']is False;n=len(t['records'])
   if case.endswith('/lin'):
    assert n>0
    rollback='rollback_'in case
    for f in t['records']:
     assert f['wall_ns']>0 and f['phase_calls'][0]==1 and f['phase_calls'][6]==0
     for i in [2,3,4,5]:assert f['phase_calls'][i]==(0 if rollback else 1),(case,i,f)
     assert f['phase_calls'][7]==int(rollback)
     assert (f['payload_bytes']==0)==rollback
     assert sum(f['phase_ns'])<=f['wall_ns']
   else:assert n==0
  else:assert not trace.exists()
  status.append(dict(role=role,case=case,exit_code=0,phase_records=n));(p/'smoke-status.json').write_text(json.dumps(dict(correctness_only=True,performance_qualified=False,runs=status),indent=2)+'\n')
 print(role,'24 cases passed',flush=True)
assert len(status)==72
assert m['source_restored']and all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
