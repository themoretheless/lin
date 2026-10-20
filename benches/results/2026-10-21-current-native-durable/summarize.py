from pathlib import Path
import json,statistics as st
p=Path(__file__).resolve().parent
out={}
for mode in ['native','durable']:
 for size in [1000,10000]:
  group=('insert_bulk_' if mode=='native' else 'durable_insert_')+('1k' if size==1000 else '10k')
  values={engine:[] for engine in ['lin','sqlite']}
  for i in range(1,7):
   d=json.loads((p/f'{mode}-{i}/run.json').read_text());assert d['status']=='complete'
   for engine in values:
    obs=[float(o['value'])/o['operations'] for o in d['observations'] if o['case']==f'compare/{group}/{engine}']
    assert len(obs)==24,(mode,size,engine,len(obs))
    values[engine].append(st.median(obs))
  out[f'{mode}-{size}']={'process_medians_ns':values,'aggregate_ns':{k:st.median(v) for k,v in values.items()},'lin_wins':sum(a<b for a,b in zip(values['lin'],values['sqlite']))}
(p/'medians.json').write_text(json.dumps(out,indent=2))
print(json.dumps(out,indent=2))
