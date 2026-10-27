from pathlib import Path
import json,statistics as st,math,collections,re
p=Path(__file__).resolve().parent;roles=['baseline','control','count32','unsorted16'];expected={f'{i:02}-{role}'for i in range(1,7)for role in roles};status=json.loads((p/'status.json').read_text());assert len(status)==24 and {r['run']for r in status}==expected and all(r['exit_code']==0 for r in status);by_status={r['run']:r for r in status};values={};phase_count=wall_count=0
shape=collections.Counter({(1,2,1,0):2,(1,3,1,0):1,(1,4,1,0):3,(1,5,1,0):2,(1,9,1,0):1,(1,544,8,0):1})
for name in sorted(expected):
 raw=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert raw==json.loads((p/f'{name}-phases.json').read_text())and len(raw)==33;phase_count+=33
 rss=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(p/f'{name}-resource.txt').read_text())[1]);assert rss==by_status[name]['peak_rss_kib']
 data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24
 for label in ['1k','10k','100k']:
  sample=[o for o in data['observations']if o['case']==f'compare/update_1row_{label}/lin'];assert len(sample)==8 and all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in sample)
 wall_count+=24
 for r in raw:
  t=r['allocation_trace'];assert t['enabled']and not t['overflow']and t['fields']==9 and t['count']==len(t['events'])==10 and t['pid']==by_status[name]['benchmark_pid']
  assert all(len(e)==6 and all(isinstance(v,int)and v>=0 for v in e)and e[5]==0 for e in t['events']);assert collections.Counter(tuple(e[:4])for e in t['events'])==shape;assert sum(e[4]for e in t['events'])<=r['stages_ns'][21]
 for n in [1000,10000,100000]:
  group=[r for r in raw if r['rows']==n];assert len(group)==11
  values[name,n]=dict(prepared_ns=st.median(r['prepared_ns']for r in group),copy_ns=st.median(r['stages_ns'][21]for r in group),node_service_ns=st.median(sum(e[4]for e in r['allocation_trace']['events']if e[1]==544)for r in group),system_service_ns=st.median(sum(e[4]for e in r['allocation_trace']['events'])for r in group))
assert phase_count==792 and wall_count==576
out={}
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns','node_service_ns','system_service_ns']:
  out[f'{n}/{metric}']={}
  for role in ['count32','unsorted16']:
   rows=[]
   for i in range(1,7):
    a,b,c=(values[f'{i:02}-{r}',n][metric]for r in ['baseline','control',role]);rows.append(dict(round=i,baseline=a,control=b,candidate=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
   gain=st.median(r['gain_percent']for r in rows);noise=st.median(abs(r['aa_percent'])for r in rows);positive=sum(r['gain_percent']>0 for r in rows);out[f'{n}/{metric}'][role]=dict(gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,processes=rows);print(n,metric,role,round(gain,3),round(noise,3),positive)
rss={role:dict(median_peak_rss_kib=st.median(by_status[f'{i:02}-{role}']['peak_rss_kib']for i in range(1,7)),processes=[dict(round=i,peak_rss_kib=by_status[f'{i:02}-{role}']['peak_rss_kib'])for i in range(1,7)])for role in roles}
(p/'summary.json').write_text(json.dumps(dict(diagnostic_only=True,engine_qualified=False,full_objective_achieved=False,phase_records=phase_count,wall_observations=wall_count,comparisons=out,peak_rss=rss),indent=2)+'\n')
