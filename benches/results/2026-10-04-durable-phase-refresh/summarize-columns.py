from pathlib import Path
import re,json,statistics as st
r=Path(__file__).resolve().parent
processes=[]
for i in range(1,4):
 run=json.loads((r/f'column-process-{i}/run.json').read_text());assert run['status']=='complete'
 groups={n:{} for n in [1000,10000]};pending=None
 for line in (r/f'column-process-{i}.log').read_text().splitlines():
  marker=re.search(r'(INSERT_PROFILE|WAL_PACK|WAL_CLASSIFY|WAL_ENCODE|WAL_COLUMN|WAL_FRAME) (.*)',line)
  if not marker:continue
  name,body=marker.groups();v={k:int(x) for k,x in re.findall(r'(\w+)=(\d+)',body)}
  if name=='WAL_FRAME':
   if pending is None:continue
   n,size=pending;assert size==v.pop('bytes');pending=None
   for k,x in v.items():groups[n].setdefault(k,[]).append(x)
   continue
  n=v.pop('rows')
  if n not in groups:continue
  if name=='WAL_ENCODE':
   size=v.pop('bytes');pending=(n,size);v['payload_bytes']=size
  if name=='WAL_COLUMN':
   field=re.search(r'field=(\w+)',body)[1];v['column_'+field+'_ns']=v.pop('elapsed_ns')
  if 'elapsed_ns' in v:v[{'WAL_PACK':'pack_ns','WAL_CLASSIFY':'classify_ns','WAL_ENCODE':'encode_ns'}[name]]=v.pop('elapsed_ns')
  for k,x in v.items():groups[n].setdefault(k,[]).append(x)
 for n,g in groups.items():
  assert len(set(map(len,g.values())))==1,(i,n,{k:len(v) for k,v in g.items()})
  assert len(g['pack_ns'])>=24
 processes.append({str(n):{'events':len(g['pack_ns']),'medians':{k:st.median(v) for k,v in g.items()},'observations':g} for n,g in groups.items()})
out={'processes':processes,'median_of_process_medians':{str(n):{k:st.median(p[str(n)]['medians'][k] for p in processes) for k in processes[0][str(n)]['medians']} for n in [1000,10000]}}
(r/'column-phase-medians.json').write_text(json.dumps(out,indent=2))
print(json.dumps(out['median_of_process_medians'],indent=2))
print('events',[[p[str(n)]['events'] for n in [1000,10000]] for p in processes])
