from pathlib import Path
import hashlib,json,re,math
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());s=json.loads((p/'smoke-status.json').read_text());cases=json.loads((p.parent/'2026-10-27-durable-mutation-baseline'/'cases.json').read_text());runs=s['runs']
assert len(runs)==72 and {(r['role'],r['case'])for r in runs}=={(role,c)for role in ['original','disabled','traced']for c in cases}
assert m['source_restored']and s['correctness_only']and not s['performance_qualified']
for f,h in m['source_sha256'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h,f
for n,h in m['instrumented_source_sha256'].items():assert hashlib.sha256((p/f'instrumented-{n}.rs').read_bytes()).hexdigest()==h
assert hashlib.sha256((p/'profile.rs').read_bytes()).hexdigest()==m['profile_sha256']
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
assert not Path('src/mutation_profile.rs').exists()
frames=0
for r in runs:
 assert r['exit_code']==0
 dest=p/'smoke'/r['role']/r['case'].replace('/','_');trace=dest.with_suffix('.phases.json');d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==1
 o=d['observations'][0];assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0
 if r['role']!='traced':assert not trace.exists()and r['phase_records']==0;continue
 t=json.loads(trace.read_text());assert t['phase_names']==['exec_program','quota','encode','crc','write','sync','checkpoint','rollback']and t['timing_qualified']is False
 assert len(t['records'])==r['phase_records'];frames+=len(t['records'])
 if not r['case'].endswith('/lin'):assert not t['records'];continue
 assert t['records'];rollback='rollback_'in r['case']
 for f in t['records']:
  assert f['wall_ns']>0 and sum(f['phase_ns'])<=f['wall_ns']and len(f['phase_ns'])==len(f['phase_calls'])==8
  assert all(isinstance(n,int)and n>=0 for n in f['phase_ns']+f['phase_calls'])
  assert f['phase_calls'][0]==1 and f['phase_calls'][6]==0 and f['phase_calls'][7]==int(rollback)
  assert all(f['phase_calls'][i]==int(not rollback)for i in [2,3,4,5])and (f['payload_bytes']==0)==rollback
assert json.loads((p/'test-status.json').read_text())==dict(exit_code=0,profile='release',features='default')
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'tests.log').read_text())];assert counts and all(x[1]==0 for x in counts)
print('PASS:',len(runs),'smoke processes;',frames,'phase records;',sum(x[0]for x in counts),'release-default tests,',sum(x[2]for x in counts),'ignored; source restored; no latency qualification')
