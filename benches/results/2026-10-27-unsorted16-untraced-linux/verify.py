from pathlib import Path
import json,hashlib,math,re
p=Path(__file__).resolve().parent
meta=json.loads((p/'provenance.json').read_text())
assert meta['instrumentation'] is False and meta['one_binary_all_roles']
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
e=json.loads((p/'effective-tunables.json').read_text());key='glibc.malloc.tcache_unsorted_limit'
assert e['baseline']==e['control']
assert e['baseline']['loader_reported_malloc'][key]==0
assert e['candidate']['loader_reported_malloc'][key]==16
assert {k:v for k,v in e['baseline']['loader_reported_malloc'].items()if k!=key}=={k:v for k,v in e['candidate']['loader_reported_malloc'].items()if k!=key}
records=json.loads((p/'process-status.json').read_text());cases=json.loads((p/'cases.json').read_text())
assert len(records)==1008 and len(cases)==28
assert len({(r['round'],r['mode'],r['case'])for r in records})==1008
status=json.loads((p/'status.json').read_text());assert len(status)==36 and all(s['exit_code']==0 and s['subprocesses']==28 for s in status)
def median(xs):
 xs=sorted(xs);return (xs[(len(xs)-1)//2]+xs[len(xs)//2])/2
med={}
for r in records:
 assert r['exit_code']==0 and r['glibc_tunables']==e[r['mode']]['glibc_tunables']
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}'
 d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and len(d['observations'])==8
 values=[]
 for o in d['observations']:
  assert o['case']==r['case'] and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
 med[r['round'],r['mode'],r['case']]=median(values)
 assert int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1])==r['peak_rss_kib']>0
summary=json.loads((p/'summary.json').read_text());passed=True;wins=0
for case in cases:
 for label,start in [('series1',1),('series2',7)]:
  gains=[];noise=[]
  for i in range(start,start+6):
   a,b,c=[med[i,m,case]for m in ['baseline','control','candidate']]
   gains.append(100*(1-c/a));noise.append(abs(100*(1-b/a)))
   if case.endswith('/lin'):wins+=c<med[i,'candidate',case[:-3]+'sqlite']
  gain,aa=median(gains),median(noise)
  ok=(all(g>0 for g in gains)and gain>aa)if case.startswith('compare/update_1row_')and case.endswith('/lin')else gain>=-max(2,aa)
  s=summary[case][label];assert s['gain_percent']==gain and s['aa_absolute_percent']==aa and s['passes']==ok
  passed &= ok
q=json.loads((p/'qualification.json').read_text())
assert q['runtime_candidate_qualified']==passed and q['candidate_sqlite_wins']==wins and q['observations']==8064 and q['full_objective_achieved']is False
assert json.loads((p/'test-status.json').read_text())['exit_code']==0
print('Independent audit PASS: 1008 processes, 8064 observations; gate:',passed)
