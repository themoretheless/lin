from pathlib import Path
import sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import role_order,orders_from_records
import json,hashlib,math,re,struct,runpy
p=Path(__file__).resolve().parent;q=p.parent/'2026-11-01-columnar-batch-wal';read=lambda f:json.loads((p/f).read_text());sha=lambda b:hashlib.sha256(b).hexdigest();frozen=read('frozen.json');assert all(sha(Path(f).read_bytes())==h for f,h in frozen.items())
m=read('provenance.json');assert m['source_restored']and m['single_binary_for_all_modes']and sha(Path(m['binary']).read_bytes())==m['binary_sha256']and sha((p/'compare.rs').read_bytes())==m['harness_sha256']and sha(Path('src/persist.rs').read_bytes())==m['candidate_persist_sha256'];assert all(sha(Path(f).read_bytes())==h for f,h in m['other_source_sha256'].items())
counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(q/'full-release.log').read_text());assert sum(int(x[0])for x in counts)==460 and sum(int(x[2])for x in counts)==2
negative=json.loads((q/'mutant-status.json').read_text());assert negative['source_restored']and negative['runtime_assertion_caught']and negative['exit_code']==101
anchor=read('timestamp-anchor.json')['fixed_timestamp_ms'];seeds=read('seed-equivalence/status.json');assert seeds['complete']and len(seeds['cases'])==2
for result in seeds['cases']:
 n=result['rows'];data={};generations={}
 for mode in ['chunked','grouped']:
  folder=p/'seed-equivalence'/f'{n}-{mode}'
  for name,info in result['files'][mode].items():
   b=(folder/name).read_bytes();assert len(b)==info['bytes']and sha(b)==info['sha256']
  rows=json.loads((folder/'expected.json').read_text());assert len(rows)==n and len({row['id']for row in rows})==n
  for row in rows:
   i=int(row['id'].split('-',1)[1]);assert row['ts']==dict(t='Time',v=anchor-(86400000 if i%2==0 else 30*86400000))
  data[mode]=rows;generations[mode]=int((folder/'expected-generation.txt').read_text())
 assert data['chunked']==data['grouped']and generations['chunked']>generations['grouped']
 for left,right in zip(data['chunked'],data['grouped']):assert [struct.pack('>f',x)for x in left['embedding']['v']]==[struct.pack('>f',x)for x in right['embedding']['v']]
traces=read('setup-traces/status.json');assert traces['complete']and len(traces['cases'])==8;assert [(r['rows'],r['mode'],r['engine'])for r in traces['cases']]==[(n,mode,engine)for n in [1000,10000]for mode in ['chunked','grouped']for engine in ['lin','sqlite_wal']];scope=runpy.run_path(str(p.parent/'2026-11-01-wal-syscall-diagnostic/audit-traces.py'))['scope']
for r in traces['cases']:
 name=f"{r['rows']}-{r['mode']}-{r['engine']}";f=p/'setup-traces'/(name+'.strace');assert r['exit_code']==0 and sha(f.read_bytes())==r['trace_sha256'];assert 'LIN_IO_FULL_VALUES_REOPEN_OK'in(p/'setup-traces'/(name+'.stdout')).read_text();text=f.read_text();calls=scope(text);assert [x['call']for x in calls if x['call']in ['fsync','fdatasync']]==(['fdatasync']if r['engine']=='lin'else['fsync'])
 lines=text.splitlines();start=[i for i,l in enumerate(lines)if 'write(2<'in l and '"LIN_IO_BEGIN\\n"'in l];assert len(start)==1;tag='/log>'if r['engine']=='lin'else'db.sqlite-wal>';before=[l for l in lines[:start[0]]if tag in l];syncs=[l for l in before if re.search(r'\b(fsync|fdatasync)\(',l)];writes=[l for l in before if re.search(r'\b(write|writev|pwrite64|pwritev)\(',l)];assert all(re.search(r'= 0$',l)for l in syncs);sizes=[int(re.search(r'= (\d+)$',l)[1])for l in writes]
 expected=(r['rows']//500+2 if r['mode']=='chunked'else 3)if r['engine']=='lin'else 2;assert len(syncs)==r['setup_wal_sync_calls']==expected and len(writes)==r['setup_wal_write_calls']and sum(sizes)==r['setup_successful_wal_write_bytes']
cases=read('cases.json');roles=read('roles.json');records=read('measurement/process-status.json');expected=[]
for rnd in range(1,13):
 for i,c in enumerate(cases[rnd%8:]+cases[:rnd%8]):
  original_index=next(j for j,x in enumerate(cases)if x['id']==c['id']);expected.extend((rnd,c['id'],role_id)for role_id in role_order(rnd,original_index,[x['id']for x in roles]))
assert len(records)==576 and [(r['round'],r['workload'],r['role'])for r in records]==expected
orders_from_records(records,[c['id']for c in cases],[x['id']for x in roles],'workload')
ids=set();med={}
def median(values):
 v=sorted(values);n=len(v);return v[n//2]if n%2 else(v[n//2-1]+v[n//2])/2
smokes=read('smoke/status.json');assert smokes['complete']and len(smokes['cases'])==32
assert [r['case']for r in smokes['cases']]==[f"compare/seed_history_{mode}_{c['operation']}_{c['size']}/{engine}"for c in cases for mode in ['chunked','grouped']for engine in ['lin','sqlite_wal']]
for r in smokes['cases']:
 out=p/'smoke'/r['output'];assert r['command']==[m['binary'],'--filter',r['case'],'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)];d=json.loads((out/'run.json').read_text());assert r['exit_code']==0 and d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==1
for r in records:
 c=next(c for c in cases if c['id']==r['workload']);role=next(x for x in roles if x['id']==r['role']);case=f"compare/seed_history_{role['mode']}_{c['operation']}_{c['size']}/{role['engine']}";out=p/'measurement'/r['output'];assert r['case']==case and r['exit_code']==0 and r['binary_sha256']==m['binary_sha256']and r['command']==[m['binary'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
 d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and d['id']not in ids and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==8;ids.add(d['id']);values=[]
 for o in d['observations']:
  v=float(o['value']);assert o['case']==case and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'and math.isfinite(v)and v>0;values.append(v)
 med[r['round'],r['workload'],r['role']]=median(values)
summary=read('measurement/summary.json')
for c in cases:
 for series,start in [('series1',1),('series2',7)]:
  pairs=[]
  for rnd in range(start,start+6):
   a,b,z,g,s,t=[med[rnd,c['id'],role]for role in ['lin_baseline','lin_control','lin_null','lin_grouped','sqlite_baseline','sqlite_control']];pairs.append(dict(round=rnd,lin_chunked_ns=a,lin_control_ns=b,lin_null_ns=z,lin_grouped_ns=g,sqlite_baseline_ns=s,sqlite_control_ns=t,lin_group_gain_percent=100*(1-g/a),lin_aa_percent=100*(1-b/a),lin_null_gain_percent=100*(1-z/a),sqlite_aa_percent=100*(1-t/s),chunked_lin_over_sqlite=a/s,grouped_lin_over_sqlite=g/s))
  r=summary[c['id']][series];assert r['pairs']==pairs;gain=median([x['lin_group_gain_percent']for x in pairs]);noise=median([abs(x['lin_aa_percent'])for x in pairs]);positive=sum(x['lin_group_gain_percent']>0 for x in pairs)
  null_gain=median([x['lin_null_gain_percent']for x in pairs]);assert r['lin_null_gain_percent']==null_gain and r['lin_null_band_passes']==(null_gain>=-max(2,noise))
  assert r['lin_group_gain_percent']==gain and r['lin_absolute_aa_percent']==noise and r['lin_positive_pairs']==positive and r['six_positive_above_aa']==(positive==6 and gain>noise)
  for field,key in [('sqlite_aa_gain_percent','sqlite_aa_percent'),('chunked_lin_over_sqlite','chunked_lin_over_sqlite'),('grouped_lin_over_sqlite','grouped_lin_over_sqlite')]:assert r[field]==median([x[key]for x in pairs])
  assert r['sqlite_absolute_aa_percent']==median([abs(x['sqlite_aa_percent'])for x in pairs])
status=read('measurement/diagnostic-status.json');assert status['complete']and status['processes']==576 and status['observations']==4608 and not status['performance_qualification']and not status['full_objective_achieved']
(p/'independent-audit.json').write_text(json.dumps(dict(complete=True,processes=576,observations=4608,same_binary_verified=True,seed_values_and_f32_bits_verified=True,linux_setup_history_verified=True,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n');print('PASS raw source, seed equivalence, setup traces, commands, observations and numerical reconstruction')
