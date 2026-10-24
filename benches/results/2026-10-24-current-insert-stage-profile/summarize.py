from pathlib import Path
import json,re,statistics as st
p=Path(__file__).resolve().parent
out={}
for mode in ['native','durable']:
 processes=[]
 for i in range(1,4):
  name=f'{mode}-{i}';d=json.loads((p/name/'run.json').read_text());assert d['status']=='complete'
  groups={1000:{},10000:{}}
  for line in (p/(name+'.log')).read_text().splitlines():
   m=re.search(r'INSERT_PHASE (.*)',line)
   if not m:continue
   v={k:int(x) for k,x in re.findall(r'(\w+)=(\d+)',m[1])};n=v.pop('rows')
   if n not in groups:continue
   for k,x in v.items():groups[n].setdefault(k,[]).append(x)
  for n,g in groups.items():
   assert len(g['build_ns'])>=24
   assert len(set(map(len,g.values())))==1
  processes.append({str(n):{'events':len(g['build_ns']),'medians':{k:st.median(v) for k,v in g.items()},'observations':g} for n,g in groups.items()})
 out[mode]={'processes':processes,'median_of_process_medians':{str(n):{k:st.median(x[str(n)]['medians'][k] for x in processes) for k in processes[0][str(n)]['medians']} for n in [1000,10000]}}
(p/'phase-medians.json').write_text(json.dumps(out,indent=2))
print(json.dumps({k:v['median_of_process_medians'] for k,v in out.items()},indent=2))
