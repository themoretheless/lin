from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());summary=json.loads((p/'summary.json').read_text());status=json.loads((p/'status.json').read_text());assert len(status)==48 and {(r['round'],r['role'])for r in status}=={(i,m)for i in range(1,13)for m in ['baseline','control','previous','candidate']};med={}
def median(xs):
 xs=sorted(xs);return (xs[(len(xs)-1)//2]+xs[len(xs)//2])/2
cases={f'{n}/{shape}/{op}'for n in [128,1024,65536,1048576]for shape in ['empty','sparse','dense']for op in ['count','empty','toggle','and','and_count','or','or_count','and_reverse','and_count_reverse','or_reverse','or_count_reverse']}
for role in ['baseline','previous','candidate']:assert hashlib.sha256((p/f'{role}.rs').read_bytes()).hexdigest()==meta['variants'][role]['source_sha256']
count=warm=0
for r in status:
 assert r['exit_code']==0
 rows=[json.loads(l)for l in(p/f'{r["round"]:02}-{r["role"]}.jsonl').read_text().splitlines()];layout=rows.pop(0);assert layout==summary['layouts'][r['role']]
 assert len(rows)==1320 and {(x['case'],x['sample'])for x in rows}=={(c,s)for c in cases for s in range(10)}
 for c in cases:
  values=[]
  for o in rows:
   if o['case']!=c:continue
   assert o['iterations']==(2048 if c.split('/')[-1]in ['count','empty','toggle']else 64)and o['elapsed_ns']>0
   assert o['warmup']==(o['sample']<2)
   if o['warmup']:warm+=1
   else:values.append(o['elapsed_ns']/o['iterations']);count+=1
  med[r['round'],r['role'],c]=median(values)
assert count==50688 and warm==12672
for c in cases:
 for label,start in [('series1',1),('series2',7)]:
  gain=[];noise=[]
  for i in range(start,start+6):
   a,b,d=[med[i,m,c]for m in ['baseline','control','candidate']];gain.append(100*(1-d/a));noise.append(abs(100*(1-b/a)))
  for pair in summary['results'][c][label]['pairs']:
   i=pair['round'];previous=med[i,'previous',c];candidate=med[i,'candidate',c]
   assert pair['previous_ns']==previous and pair['candidate_ns']==candidate
   assert pair['gain_vs_previous_percent']==100*(1-candidate/previous)
  s=summary['results'][c][label];assert s['gain_percent']==median(gain)and s['aa_absolute_percent']==median(noise)and s['positive_pairs']==sum(g>0 for g in gain)
assert summary['diagnostic_only']and summary['full_objective_achieved']is False
print('Independent audit PASS: 48 processes, 132 cases, 50688 measured + 12672 warmup observations')
