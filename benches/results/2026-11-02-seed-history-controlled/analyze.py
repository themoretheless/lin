from pathlib import Path
import json,math,statistics,sys
p=Path(sys.argv[1]).resolve()if len(sys.argv)>1 else Path(__file__).resolve().parent;read=lambda f:json.loads((p/f).read_text());root=p/'measurement';cases=read('cases.json');roles=read('roles.json');records=read('measurement/process-status.json');assert len(cases)==8 and len(roles)==5 and len(records)==480
expected=[]
for rnd in range(1,13):
 for i,c in enumerate(cases[rnd%8:]+cases[:rnd%8]):
  shift=(rnd+i)%5;expected.extend((rnd,c['id'],r['id'])for r in roles[shift:]+roles[:shift])
assert [(r['round'],r['workload'],r['role'])for r in records]==expected
med={};ids=set();observations=0
for r in records:
 c=next(c for c in cases if c['id']==r['workload']);role=next(x for x in roles if x['id']==r['role']);case=f"compare/seed_history_{role['mode']}_{c['operation']}_{c['size']}/{role['engine']}";assert r['exit_code']==0 and r['case']==case
 d=json.loads((root/r['output']/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==case and len(d['observations'])==8 and d['id']not in ids;ids.add(d['id']);values=[]
 for o in d['observations']:
  v=float(o['value']);assert o['case']==case and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'and math.isfinite(v)and v>0;values.append(v);observations+=1
 med[r['round'],r['workload'],r['role']]=statistics.median(values)
assert observations==3840
summary={}
for c in cases:
 series={}
 for name,start in [('series1',1),('series2',7)]:
  pairs=[]
  for rnd in range(start,start+6):
   a,b,g,s,t=[med[rnd,c['id'],role]for role in ['lin_baseline','lin_control','lin_grouped','sqlite_baseline','sqlite_control']];pairs.append(dict(round=rnd,lin_chunked_ns=a,lin_control_ns=b,lin_grouped_ns=g,sqlite_baseline_ns=s,sqlite_control_ns=t,lin_group_gain_percent=100*(1-g/a),lin_aa_percent=100*(1-b/a),sqlite_aa_percent=100*(1-t/s),chunked_lin_over_sqlite=a/s,grouped_lin_over_sqlite=g/s))
  median=lambda key:statistics.median(x[key]for x in pairs);gain=median('lin_group_gain_percent');noise=statistics.median(abs(x['lin_aa_percent'])for x in pairs);positive=sum(x['lin_group_gain_percent']>0 for x in pairs)
  series[name]=dict(pairs=pairs,lin_group_gain_percent=gain,lin_absolute_aa_percent=noise,lin_positive_pairs=positive,six_positive_above_aa=positive==6 and gain>noise,sqlite_aa_gain_percent=median('sqlite_aa_percent'),sqlite_absolute_aa_percent=statistics.median(abs(x['sqlite_aa_percent'])for x in pairs),chunked_lin_over_sqlite=median('chunked_lin_over_sqlite'),grouped_lin_over_sqlite=median('grouped_lin_over_sqlite'))
 summary[c['id']]=series
(root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(root/'diagnostic-status.json').write_text(json.dumps(dict(complete=True,processes=480,observations=3840,performance_qualification=False,runtime_adopted=False,full_objective_achieved=False),indent=2)+'\n')
lines=['# Controlled seed transaction history','','One executable, fixed fixture values. Different seed transactions also change generation, allocation and cache history. These descriptive results do not identify a single physical cause or qualify production adoption. Every original outcome is retained.','','|Case|Series|Lin grouped gain %|Lin absolute A/A %|Positive pairs|SQLite absolute A/A %|Chunked Lin/SQLite|Grouped Lin/SQLite|','|---|---|---:|---:|---:|---:|---:|---:|']
for case,series in summary.items():
 for name,r in series.items():lines.append(f"|{case}|{name}|{r['lin_group_gain_percent']:.3f}|{r['lin_absolute_aa_percent']:.3f}|{r['lin_positive_pairs']}/6|{r['sqlite_absolute_aa_percent']:.3f}|{r['chunked_lin_over_sqlite']:.6f}|{r['grouped_lin_over_sqlite']:.6f}|")
(root/'report.md').write_text('\n'.join(lines)+'\n');print('480 fresh diagnostic processes / 3840 observations analyzed; no adoption gate')
