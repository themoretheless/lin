from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;status=json.loads((p/'status.json').read_text());expected={f'{i:02}-{role}'for i in range(1,7)for role in ['baseline','control','trace']};assert len(status)==18 and {r['run']for r in status}==expected and all(r['exit_code']==0 for r in status);values={};raw_groups={};phase_count=wall_count=0
for name in sorted(expected):
 rows=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert rows==json.loads((p/f'{name}-phases.json').read_text())and len(rows)==33;phase_count+=33
 data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24
 for label in ['1k','10k','100k']:
  sample=[o for o in data['observations']if o['case']==f'compare/update_1row_{label}/lin'];assert len(sample)==8 and all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in sample)
 wall_count+=24
 for r in rows:
  trace=r['allocation_trace'];assert trace['enabled']==name.endswith('-trace')and not trace['overflow']and trace['count']==len(trace['events'])<=64
  assert r['diagnostic_only']and r['kind']=='update'and len(r['stages_ns'])==25 and r['stages_ns'][21]>0
  for e in trace['events']:assert len(e)==6 and all(isinstance(v,int)and v>=0 for v in e)and e[0]in [1,2,3,4]and e[2]>0 and e[2]&(e[2]-1)==0 and e[5]==0
  assert sum(e[4]for e in trace['events'])<=r['stages_ns'][21]
 for n in [1000,10000,100000]:
  group=[r for r in rows if r['rows']==n];assert len(group)==11;raw_groups[name,n]=group
  values[name,n]={'prepared_ns':st.median(r['prepared_ns']for r in group),'copy_ns':st.median(r['stages_ns'][21]for r in group)}
assert phase_count==594 and wall_count==432
comparisons={};layouts={}
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns']:
  groups=[]
  for i in range(1,7):
   a,b,c=(values[f'{i:02}-{role}',n][metric]for role in ['baseline','control','trace']);groups.append(dict(round=i,baseline=a,control=b,trace=c,trace_overhead_percent=100*(c/a-1),aa_percent=100*(b/a-1)))
  comparisons[f'{n}/{metric}']=dict(trace_overhead_percent=st.median(r['trace_overhead_percent']for r in groups),aa_absolute_percent=st.median(abs(r['aa_percent'])for r in groups),processes=groups)
 process_rows=[]
 for i in range(1,7):
  group=raw_groups[f'{i:02}-trace',n];shapes={tuple(e[:4])for r in group for e in r['allocation_trace']['events']};per_layout={}
  for shape in sorted(shapes):
   key='/'.join(map(str,shape));per_layout[key]=dict(count=st.median(sum(tuple(e[:4])==shape for e in r['allocation_trace']['events'])for r in group),service_ns=st.median(sum(e[4]for e in r['allocation_trace']['events']if tuple(e[:4])==shape)for r in group))
  process_rows.append(dict(process=i,copy_ns=values[f'{i:02}-trace',n]['copy_ns'],system_service_ns=st.median(sum(e[4]for e in r['allocation_trace']['events'])for r in group),remainder_ns=st.median(r['stages_ns'][21]-sum(e[4]for e in r['allocation_trace']['events'])for r in group),fields=st.median(r['allocation_trace']['fields']for r in group),event_count=st.median(r['allocation_trace']['count']for r in group),layouts=per_layout))
 keys=set().union(*(r['layouts']for r in process_rows));layouts[str(n)]=dict(copy_ns=st.median(r['copy_ns']for r in process_rows),system_service_ns=st.median(r['system_service_ns']for r in process_rows),remainder_ns=st.median(r['remainder_ns']for r in process_rows),event_count=st.median(r['event_count']for r in process_rows),fields=st.median(r['fields']for r in process_rows),layouts={k:{field:st.median(r['layouts'].get(k,{}).get(field,0)for r in process_rows)for field in ['count','service_ns']}for k in sorted(keys)},processes=process_rows)
 print(n,'trace copy/service/remainder us',*[round(layouts[str(n)][k]/1000,3)for k in ['copy_ns','system_service_ns','remainder_ns']])
(p/'summary.json').write_text(json.dumps(dict(diagnostic_only=True,engine_qualified=False,full_objective_achieved=False,phase_records=phase_count,wall_observations=wall_count,comparisons=comparisons,allocations=layouts),indent=2)+'\n')
