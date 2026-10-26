from pathlib import Path
import json,hashlib,math,statistics
p=Path(__file__).resolve().parent
read=lambda name:json.loads((p/name).read_text())
sha=lambda file:hashlib.sha256(Path(file).read_bytes()).hexdigest()
meta=read('provenance.json');assert meta['baseline']['base_commit'].startswith('f19f41a4')
for v,source in [('baseline','before-exec.rs'),('candidate','candidate-exec.rs')]:assert sha(p/source)==meta[v]['source_sha256']['src/exec.rs']
assert all(x==0 for x in read('checks.json').values())
final=read('final-functional.json');assert all(final[k]==0 for k in ['final-workspace','no-default','optional','final-release']);assert final['final_binary_equal_measured']
assert sha(p/'tests-prepared-grouped-count.rs')==sha('tests/prepared_grouped_count.rs')
all_cases={};observation_count=0
for prefix,count in [('',96),('aa-controls/',48)]:
 status=read(prefix+'status.json');assert len(status)==count and all(v==0 for v in status.values())
 for name in status:
  d=read(prefix+name+'/run.json');assert d['status']=='complete';wall={}
  for o in d['observations']:
   if o['metric']!='wall':continue
   assert o['availability']['state']=='available' and o['operations']>0
   value=float(o['value'])/o['operations'];assert math.isfinite(value) and value>0
   wall.setdefault(o['case'],[]).append(value);observation_count+=1
  assert all(len(v)==24 for v in wall.values())
  all_cases[prefix+name]={k:statistics.median(v) for k,v in wall.items()}
summary=read('summary.json');wins=0;comparisons=0
for key,x in summary.items():
 series,case=key.split('/',1);family='grouped' if '/filter' in case else 'point' if '/point_get/' in case else 'substring' if '/text_substr/' in case else 'join'
 for i in range(1,7):
  a=all_cases[f'{series}-{family}-baseline-{i}'][case];b=all_cases[f'{series}-{family}-candidate-{i}'][case]
  assert x['baseline_ns'][i-1]==a and x['candidate_ns'][i-1]==b
  assert abs(x['pair_gains'][i-1]-100*(a-b)/a)<1e-9
 assert x['paired_gain_percent']==statistics.median(x['pair_gains'])
for series in [1,2]:
 for case in ['filter_eq','filter_range']:
  lin=summary[f's{series}/compare/{case}/lin']['candidate_ns']
  for peer in ['sqlite','duckdb']:
   values=summary[f's{series}/compare/{case}/{peer}']['candidate_ns']
   for a,b in zip(lin,values):wins+=a<b;comparisons+=1
assert wins==comparisons==48
resources=read('resources.json');assert len(resources)==6 and all(x['max_rss_bytes']>0 for x in resources)
aa=read('aa-controls/provenance.json');assert aa['binary_sha256']==meta['baseline']['binary_sha256']
ver=dict(functional_gates=True,benchmark_processes=144,resource_processes=6,wall_observations=observation_count,target_peer_comparisons_won=wins,target_peer_comparisons=comparisons,test_sha256=sha(p/'tests-prepared-grouped-count.rs'),decision='accept index-covered fixed-text-group prepared dispatch; full peer objective incomplete')
(p/'verification.json').write_text(json.dumps(ver,indent=2)+'\n');print(json.dumps(ver))
