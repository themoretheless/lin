from pathlib import Path
import json,statistics
r=Path(__file__).resolve().parent
peers=['sqlite','duckdb','postgres','mysql','mongo','pandas'];cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']
summary={};ratios={p:[] for p in peers}
for n in [1000,10000,100000]:
 d=json.loads((r/f'rows-{n}/run.json').read_text());assert d['repeats']==3 and d['samples']==8
 assert set(d['engines'])==set(['lin',*peers,'mssql','kusto'])
 losses=[];wins=0
 for peer in peers:
  for case in cases:
   k=f'{case}/{peer}';lk=f'{case}/lin'
   obs=[o for o in d['observations'] if o['case']==k]
   assert len(obs)==3 and all(len(o['samples_ns'])==8 for o in obs),(n,k)
   measured=statistics.median(statistics.median(o['samples_ns']) for o in obs)
   assert measured==d['medians_ns'][k]
   ratio=measured/d['medians_ns'][lk];ratios[peer].append(ratio)
   if ratio>1:wins+=1
   else:losses.append({'case':k,'peer_over_lin':ratio})
 summary[str(n)]={'status':d['status'],'wins':wins,'comparisons':36,'losses':losses,'errors':d['errors'],'versions':d['versions']}
 print(n,wins,'/36',losses)
out={'datasets':summary,'minimum_peer_over_lin':{k:min(v) for k,v in ratios.items()}}
(r/'overview.json').write_text(json.dumps(out,indent=2))
print(out['minimum_peer_over_lin'])
