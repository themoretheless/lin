from pathlib import Path
import json,hashlib,math,collections
p=Path(__file__).resolve().parent
def median(values):
 v=sorted(values);assert v;middle=len(v)//2;return v[middle]if len(v)%2 else(v[middle-1]+v[middle])/2
meta=json.loads((p/'input-provenance.json').read_text())
for name,digest in meta['diagnostic_files_sha256'].items():assert hashlib.sha256((p/name).read_bytes()).hexdigest()==digest
for name,digest in meta['candidate_sha256'].items():assert hashlib.sha256((p/f'map-source-{name}.rs').read_bytes()).hexdigest()==digest
provenance=json.loads((p/'provenance.json').read_text())
for path,digest in provenance['instrumented_sha256'].items():assert hashlib.sha256((p/('instrumented-'+Path(path).name)).read_bytes()).hexdigest()==digest
for path,digest in provenance['before_sha256'].items():assert digest==(meta['compare_sha256']if path=='benches/compare.rs'else meta['source_sha256'][path])
expected={f'{i:02}-{role}'for i in range(1,7)for role in ['baseline','control','trace']};status=json.loads((p/'status.json').read_text());assert len(status)==18 and {r['run']for r in status}==expected and len({(r['pid'],r['started_unix'])for r in status})==18
assert all(r['exit_code']==0 and r['role']==r['run'].split('-')[1]and r['elapsed_seconds']>0 for r in status)
raw={};med={};phases=walls=events_total=0
shapes=collections.Counter({(1,2,1,0):2,(1,3,1,0):1,(1,4,1,0):3,(1,5,1,0):2,(1,9,1,0):1,(1,544,8,0):1})
for name in sorted(expected):
 rows=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert rows==json.loads((p/f'{name}-phases.json').read_text())and len(rows)==33;phases+=len(rows)
 data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24
 for label in ['1k','10k','100k']:
  values=[o for o in data['observations']if o['case']==f'compare/update_1row_{label}/lin'];assert len(values)==8 and all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in values)
 walls+=24
 for r in rows:
  assert r['diagnostic_only']and r['kind']=='update'and len(r['stages_ns'])==25 and all(isinstance(v,int)and v>=0 for v in r['stages_ns'])and r['prepared_ns']>0 and r['stages_ns'][21]>0
  t=r['allocation_trace'];assert t['fields']==9 and t['enabled']==name.endswith('-trace')and not t['overflow']and t['count']==len(t['events'])<=64
  assert all(len(e)==6 and all(isinstance(v,int)and v>=0 for v in e)and e[5]==0 for e in t['events'])
  if t['enabled']:assert collections.Counter(tuple(e[:4])for e in t['events'])==shapes;events_total+=len(t['events'])
  else:assert t['count']==0
  assert sum(e[4]for e in t['events'])<=r['stages_ns'][21]
 for n in [1000,10000,100000]:
  group=[r for r in rows if r['rows']==n];assert len(group)==11;raw[name,n]=group;med[name,n]=dict(prepared_ns=median(r['prepared_ns']for r in group),copy_ns=median(r['stages_ns'][21]for r in group))
s=json.loads((p/'summary.json').read_text());assert s['diagnostic_only']and s['engine_qualified']is False and s['full_objective_achieved']is False and s['phase_records']==phases==594 and s['wall_observations']==walls==432
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns']:
  pairs=[]
  for i in range(1,7):
   a,b,c=(med[f'{i:02}-{role}',n][metric]for role in ['baseline','control','trace']);pairs.append(dict(round=i,baseline=a,control=b,trace=c,trace_overhead_percent=100*(c/a-1),aa_percent=100*(b/a-1)))
  assert s['comparisons'][f'{n}/{metric}']==dict(trace_overhead_percent=median(r['trace_overhead_percent']for r in pairs),aa_absolute_percent=median(abs(r['aa_percent'])for r in pairs),processes=pairs)
 processes=[]
 for i in range(1,7):
  group=raw[f'{i:02}-trace',n];layout={}
  for shape in sorted(shapes):layout['/'.join(map(str,shape))]=dict(count=median(sum(tuple(e[:4])==shape for e in r['allocation_trace']['events'])for r in group),service_ns=median(sum(e[4]for e in r['allocation_trace']['events']if tuple(e[:4])==shape)for r in group))
  processes.append(dict(process=i,copy_ns=med[f'{i:02}-trace',n]['copy_ns'],system_service_ns=median(sum(e[4]for e in r['allocation_trace']['events'])for r in group),remainder_ns=median(r['stages_ns'][21]-sum(e[4]for e in r['allocation_trace']['events'])for r in group),fields=9,event_count=10,layouts=layout))
 rebuilt={key:median(r[key]for r in processes)for key in ['copy_ns','system_service_ns','remainder_ns','fields','event_count']};rebuilt['processes']=processes;rebuilt['layouts']={key:{field:median(r['layouts'][key][field]for r in processes)for field in ['count','service_ns']}for key in processes[0]['layouts']};assert s['allocations'][str(n)]==rebuilt
assert events_total==1980
print(json.dumps(dict(processes=18,phase_records=phases,wall_observations=walls,traced_events=events_total,all_layout_distributions_verified=True,all_medians_verified=True,source_hashes_verified=True,diagnostic_only=True,engine_qualified=False,full_objective_achieved=False),indent=2))
