from pathlib import Path
import json,statistics as st
p=Path(__file__).resolve().parent;out={}
for n in [1000,10000,100000]:
 for metric in ['ns_per_delete']:
  pairs=[]
  for i in range(1,7):
   values={}
   for mode in ['baseline','control','candidate']:
    rows=[json.loads(s)for s in(p/f'{i:02}-{mode}.jsonl').read_text().splitlines()];assert len(rows)==48 and all(r['exact_readback']for r in rows)
    v=[r[metric]for r in rows if r['rows']==n and metric in r];assert len(v)==16;values[mode]=st.median(v)
   pairs.append(dict(round=i,**values,gain=100*(1-values['candidate']/values['baseline']),aa=100*(1-values['control']/values['baseline'])))
  out[f'{n}/{metric}']=dict(gain_percent=st.median(v['gain']for v in pairs),aa_absolute_percent=st.median(abs(v['aa'])for v in pairs),positive_pairs=sum(v['gain']>0 for v in pairs),processes=pairs)
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
for n,x in out.items():print(n,round(x['gain_percent'],3),round(x['aa_absolute_percent'],3),x['positive_pairs'])
