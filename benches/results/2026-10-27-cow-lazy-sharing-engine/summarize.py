from pathlib import Path
import json,statistics as st,math,re
p=Path(__file__).resolve().parent;cases=json.loads((p/'cases.json').read_text());records=json.loads((p/'process-status.json').read_text());assert len(cases)==37 and len(records)==1332 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,m,c)for i in range(1,13)for m in ['baseline','control','candidate']for c in cases};assert all(r['exit_code']==0 for r in records);med={};count=0
for r in records:
 name=r['case'].replace('/','_');data=json.loads((p/f'{r["round"]:02}-{r["mode"]}'/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==1 and len(data['observations'])==8;sample=[]
 for o in data['observations']:assert o['case']==r['case']and o['operations']==1 and o['metric']=='wall';v=float(o['value']);assert math.isfinite(v)and v>0;sample.append(v);count+=1
 resource=p/f'{r["round"]:02}-{r["mode"]}'/(name+'-resource.txt');rss=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',resource.read_text())[1]);assert rss==r['peak_rss_kib'];med[r['round'],r['mode'],r['case']]=st.median(sample)
assert count==10656;out={};qualified=True;wins=0
for case in cases:
 out[case]={}
 for label,rounds in [('series1',range(1,7)),('series2',range(7,13))]:
  rows=[]
  for i in rounds:
   a,b,c=(med[i,m,case]for m in ['baseline','control','candidate']);r=dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a))
   if case.endswith('/lin'):r['candidate_sqlite_ns']=med[i,'candidate',case[:-3]+'sqlite'];r['wins_sqlite']=c<r['candidate_sqlite_ns'];wins+=r['wins_sqlite']
   rows.append(r)
  gain=st.median(r['gain_percent']for r in rows);noise=st.median(abs(r['aa_percent'])for r in rows);positive=sum(r['gain_percent']>0 for r in rows);target=case.startswith('compare/update_1row_')and case.endswith('/lin');passed=(positive==6 and gain>noise)if target else(gain>=-max(2,noise));qualified &= passed
  q=dict(pairs=6,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,processes=rows)
  if case.endswith('/lin'):q['sqlite_wins']=sum(r['wins_sqlite']for r in rows)
  q['peak_rss_kib']={m:st.median(r['peak_rss_kib']for r in records if r['round']in rounds and r['mode']==m and r['case']==case)for m in ['baseline','control','candidate']};out[case][label]=q
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(dict(source_candidate_qualified=qualified,complete_rounds=12,processes=1332,observations=count,candidate_sqlite_wins=wins,candidate_sqlite_comparisons=204,full_objective_achieved=False,scope='production main vs lazy Inline/Shared Row promotion and existing inplace/bounded-prefix experimental source; public Row newtype break; native mutation/insert/COUNT controls; durable mutations and broader peers unqualified'),indent=2)+'\n');print('complete37-case gate',qualified,'SQLite',wins,'/204')
