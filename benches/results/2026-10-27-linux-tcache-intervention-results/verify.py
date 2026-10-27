from pathlib import Path
import json,hashlib,math,collections,re
p=Path(__file__).resolve().parent
def median(xs):
 a=sorted(xs);assert a;n=len(a)//2;return a[n]if len(a)%2 else(a[n-1]+a[n])/2
meta=json.loads((p/'input-provenance.json').read_text())
for name,digest in meta['diagnostic_files_sha256'].items():assert hashlib.sha256((p/name).read_bytes()).hexdigest()==digest
for name,digest in meta['candidate_sha256'].items():assert hashlib.sha256((p/f'map-source-{name}.rs').read_bytes()).hexdigest()==digest
provenance=json.loads((p/'provenance.json').read_text())
for name,digest in provenance['instrumented_sha256'].items():assert hashlib.sha256((p/('instrumented-'+Path(name).name)).read_bytes()).hexdigest()==digest
for name,digest in provenance['before_sha256'].items():assert digest==(meta['compare_sha256']if name=='benches/compare.rs'else meta['source_sha256'][name])
roles=['baseline','control','count32','unsorted16'];expected={f'{i:02}-{role}'for i in range(1,7)for role in roles};status=json.loads((p/'status.json').read_text());assert len(status)==24 and {r['run']for r in status}==expected and len({(r['benchmark_pid'],r['started_unix'])for r in status})==24
by_status={r['run']:r for r in status};environment=json.loads((p/'environment.json').read_text());assert environment['libc']==['glibc','2.39'];base=environment['allocator_environment']['GLIBC_TUNABLES']
shape=collections.Counter({(1,2,1,0):2,(1,3,1,0):1,(1,4,1,0):3,(1,5,1,0):2,(1,9,1,0):1,(1,544,8,0):1});values={};phase_count=wall_count=events_total=0
for name in sorted(expected):
 entry=by_status[name];role=name.split('-',1)[1];assert entry['exit_code']==0 and entry['role']==role and entry['elapsed_seconds']>0 and entry['benchmark_pid']!=entry['wrapper_pid']
 if role in ['baseline','control']:assert entry['glibc_tunables']==base
 else:
  key,value=('glibc.malloc.tcache_count','32')if role=='count32'else('glibc.malloc.tcache_unsorted_limit','16');expected_string=':'.join([s for s in(base or'').split(':')if s and s.split('=',1)[0]!=key]+[key+'='+value]);assert entry['glibc_tunables']==expected_string
 rss=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(p/f'{name}-resource.txt').read_text())[1]);assert rss==entry['peak_rss_kib']and rss>0
 raw=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert raw==json.loads((p/f'{name}-phases.json').read_text())and len(raw)==33;phase_count+=len(raw)
 d=json.loads((p/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==3 and len(d['observations'])==24
 for label in ['1k','10k','100k']:
  observations=[o for o in d['observations']if o['case']==f'compare/update_1row_{label}/lin'];assert len(observations)==8 and all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in observations)
 wall_count+=24
 for r in raw:
  assert r['diagnostic_only']and r['kind']=='update'and len(r['stages_ns'])==25 and all(isinstance(v,int)and v>=0 for v in r['stages_ns'])and r['prepared_ns']>0 and r['stages_ns'][21]>0
  t=r['allocation_trace'];assert t['enabled']and not t['overflow']and t['fields']==9 and t['pid']==entry['benchmark_pid']and t['count']==len(t['events'])==10
  assert all(len(e)==6 and all(isinstance(v,int)and v>=0 for v in e)and e[5]==0 for e in t['events']);assert collections.Counter(tuple(e[:4])for e in t['events'])==shape;events_total+=len(t['events']);assert sum(e[4]for e in t['events'])<=r['stages_ns'][21]
 for n in [1000,10000,100000]:
  rows=[r for r in raw if r['rows']==n];assert len(rows)==11;values[name,n]=dict(prepared_ns=median(r['prepared_ns']for r in rows),copy_ns=median(r['stages_ns'][21]for r in rows),node_service_ns=median(sum(e[4]for e in r['allocation_trace']['events']if e[1]==544)for r in rows),system_service_ns=median(sum(e[4]for e in r['allocation_trace']['events'])for r in rows))
s=json.loads((p/'summary.json').read_text());assert s['diagnostic_only']and s['engine_qualified']is False and s['full_objective_achieved']is False and s['phase_records']==phase_count==792 and s['wall_observations']==wall_count==576
assert set(s['comparisons'])=={f'{n}/{metric}'for n in [1000,10000,100000]for metric in ['prepared_ns','copy_ns','node_service_ns','system_service_ns']}
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns','node_service_ns','system_service_ns']:
  for role in ['count32','unsorted16']:
   rows=[]
   for i in range(1,7):
    a,b,c=(values[f'{i:02}-{r}',n][metric]for r in ['baseline','control',role]);rows.append(dict(round=i,baseline=a,control=b,candidate=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
   assert s['comparisons'][f'{n}/{metric}'][role]==dict(gain_percent=median(r['gain_percent']for r in rows),aa_absolute_percent=median(abs(r['aa_percent'])for r in rows),positive_pairs=sum(r['gain_percent']>0 for r in rows),processes=rows)
for role in roles:assert s['peak_rss'][role]==dict(median_peak_rss_kib=median(by_status[f'{i:02}-{role}']['peak_rss_kib']for i in range(1,7)),processes=[dict(round=i,peak_rss_kib=by_status[f'{i:02}-{role}']['peak_rss_kib'])for i in range(1,7)])
assert events_total==7920
print(json.dumps(dict(processes=24,phase_records=phase_count,wall_observations=wall_count,traced_events=events_total,configurations_verified=True,all_medians_verified=True,peak_rss_verified=True,source_hashes_verified=True,diagnostic_only=True,engine_qualified=False,full_objective_achieved=False),indent=2))
