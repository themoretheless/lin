import json,statistics as st,hashlib
from pathlib import Path
p=Path(__file__).resolve().parent
out={}
for mode in ['native','durable']:
 for case in json.loads((p/f'repeat-{mode}-baseline-1/run.json').read_text())['cases']:
  key=case['id']
  def med(v,i):
   d=json.loads((p/f'repeat-{mode}-{v}-{i}/run.json').read_text());assert d['status']=='complete'
   return st.median(float(x['value'])/x['operations'] for x in d['observations'] if x['case']==key)
  a=[med('baseline',i) for i in range(1,7)];b=[med('candidate',i) for i in range(1,7)]
  out[key]=dict(baseline_ns=a,candidate_ns=b,baseline_median_ns=st.median(a),candidate_median_ns=st.median(b),paired_gain_percent=st.median(100*(x-y)/x for x,y in zip(a,b)),wins=sum(y<x for x,y in zip(a,b)))
  print(key,round(out[key]['paired_gain_percent'],3),out[key]['wins'])
(p/'repeat-medians.json').write_text(json.dumps(out,indent=2))
(p/'repeat-source-sha256.json').write_text(json.dumps({f:hashlib.sha256((p/f).read_bytes()).hexdigest() for f in ['before-embed.rs','candidate-embed.rs']},indent=2))
