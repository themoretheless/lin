from pathlib import Path
import subprocess,json,os,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
cases=[f'compare/durable_{op}_1row_{size}/lin'for size in ['1k','10k']for op in ['update','delete','commit','rollback']]+['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin']
assert len(cases)==10
status=[]
for case in cases:
 for role in ['original','disabled','traced']:
  binary=m['binaries']['original'if role=='original'else'instrumented'];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
  name=case.replace('/','_')+'-'+role;output=p/'smoke'/name;output.parent.mkdir(exist_ok=True);trace=output.with_suffix('.trace.json')
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C',LIN_MUTATION_TRACE='1'if role=='traced'else'0',LIN_MUTATION_TRACE_PATH=str(trace));env.pop('LIN_BENCH_MIXED_CASE',None)
  with output.with_suffix('.log').open('w')as log:r=subprocess.run([binary['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(output)],env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,(case,role)
  raw=json.loads((output/'run.json').read_text());assert raw['status']=='complete'and len(raw['cases'])==1 and raw['cases'][0]['id']==case and len(raw['observations'])==1
  if role=='traced':
   data=json.loads(trace.read_text());assert data['records'] and data['phase_names']==['exec_program','quota','encode','crc','write','sync','checkpoint','rollback']
   for frame in data['records']:
    assert frame['wall_ns']>0 and len(frame['phase_calls'])==8 and len(frame['phase_ns'])==8
    rollback='rollback'in case;normal='scalar_normal'in case
    for idx in [2,3,4]:assert frame['phase_calls'][idx]==(0 if rollback else 1),(case,frame)
    assert frame['phase_calls'][5]==(0 if rollback or normal else 1),(case,frame)
    assert frame['phase_calls'][6]==0 and frame['phase_calls'][7]==int(rollback),(case,frame)
    assert (frame['payload_bytes']==0)==rollback
  else:assert not trace.exists()
  status.append(dict(case=case,role=role,exit_code=0));(p/'smoke-status.json').write_text(json.dumps(status,indent=2)+'\n')
print('30 current-source correctness/trace smokes passed')
