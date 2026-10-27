from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;frozen=p.parent/'2026-10-27-cow-broad-engine';load=lambda n:json.loads((p/n).read_text());test=load('test-status.json');a=load('allocation-status.json');mut=load('mutant-status.json')
assert test['exit_code']==0 and test['source_restored']and a['source_restored']and not test['performance_qualified']and not a['performance_qualified']and test['public_row_newtype_break']
for meta in [test,a]:
 for f,h in meta['source_sha256'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h,f
for n,h in test['candidate_sha256'].items():assert hashlib.sha256(((p if n in ['exec','store']else frozen)/f'candidate-{n}.rs').read_bytes()).hexdigest()==h
assert hashlib.sha256((p/'allocation-probe.rs').read_bytes()).hexdigest()==a['probe_sha256']
assert set(a['roles'])=={'main','frozen_cow','lazy_cow'}
for role,m in a['roles'].items():
 for n,h in m['source_sha256'].items():
  expected=a['source_sha256'][f'src/{n}.rs']if role=='main'else hashlib.sha256(((p if role=='lazy_cow'and n in ['exec','store']else frozen)/f'candidate-{n}.rs').read_bytes()).hexdigest();assert h==expected
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'tests.log').read_text())];assert sum(x[0]for x in counts)==419 and sum(x[2]for x in counts)==2 and all(x[1]==0 for x in counts)
assert mut['expected_failure']and mut['runtime_assertion_caught']and mut['exit_code']!=0 and 'test result: FAILED'in(p/'mutant.log').read_text()
cases=['empty_row_new_drop','one_cell_construct_drop','nine_cell_construct_drop','unpromoted_row_clone_drop','returned_update_row_clone_drop','returned_update_row_detach_drop'];spec={'main':[(0,0),(3,593),(11,1014),(10,582),(10,582),(10,582)],'frozen_cow':[(1,40),(4,633),(12,1054),(0,0),(0,0),(11,622)],'lazy_cow':[(0,0),(3,593),(11,1014),(10,582),(0,0),(11,622)]};sizes={'main':24,'frozen_cow':8,'lazy_cow':32};ops=0
for role in spec:
 for i in range(1,4):
  d=load(f'allocation-{role}-{i}.json');assert d['row_size_bytes']==sizes[role]and d['map_size_bytes']==24 and d['fixture_fields']==9
  assert len(d['records'])==18 and {(r['case'],r['repeat'])for r in d['records']}=={(c,j)for c in cases for j in range(3)}
  for r in d['records']:
   n,b=spec[role][cases.index(r['case'])];assert r['operations']==1000 and r['allocations']==r['deallocations']==n*1000 and r['requested_bytes']==r['deallocated_bytes']==b*1000;ops+=r['operations']
  assert not(p/f'allocation-{role}-{i}.stderr.log').read_text()
assert ops==162000
assert not Path('tests/cow_lazy_owning_probe.rs').exists()and not Path('examples/cow_lazy_allocation_probe.rs').exists()
print('PASS:419 tests/2 ignored; cross-representation Eq mutant caught;9 allocation processes/162000 primitive operations; source restored; no latency qualification')
