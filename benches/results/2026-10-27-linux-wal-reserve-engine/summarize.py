from pathlib import Path
import json,math,re,statistics as st
p=Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
cases=load('cases.json');records=load('process-status.json');assert len(cases)==len(set(cases))==61
expected={(i,m,c)for i in range(1,13)for m in ['baseline','control','candidate']for c in cases}
assert len(records)==2196 and {(r['round'],r['mode'],r['case'])for r in records}==expected
med={};rss={};count=0
for r in records:
 assert r['exit_code']==0
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}';d=json.loads((base/name/'run.json').read_text())
 assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
 values=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);count+=1
 key=r['round'],r['mode'],r['case'];med[key]=st.median(values)
 resident=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert resident==r['peak_rss_kib']>0;rss[key]=resident
assert count==17568
summary={};qualified=True;comparisons=[]
for case in cases:
 target=case.endswith('/lin')and any(case.startswith('compare/durable_'+op+'_1row_')for op in ['update','delete','commit'])
 summary[case]={}
 for label,start in [('series1',1),('series2',7)]:
  rounds=range(start,start+6);rows=[]
  for i in rounds:
   a,b,c=[med[i,m,case]for m in ['baseline','control','candidate']]
   rows.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
  gain=st.median(r['gain_percent']for r in rows);noise=st.median(abs(r['aa_percent'])for r in rows);positive=sum(r['gain_percent']>0 for r in rows)
  passed=(positive==6 and gain>noise)if target else gain>=-max(2,noise);qualified &=passed
  summary[case][label]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,process_pairs=rows,peak_rss_kib={m:st.median(rss[i,m,case]for i in rounds)for m in ['baseline','control','candidate']})
  if case.endswith('/lin'):
   for peer in ['sqlite','sqlite_delete','sqlite_wal','duckdb']:
    peer_case=case[:-3]+peer
    if peer_case not in cases:continue
    ratios=[med[i,'candidate',case]/med[i,'candidate',peer_case]for i in rounds]
    comparisons.append(dict(case=case,peer=peer,series=label,pairs=6,median_ratio=st.median(ratios),lin_wins=sum(r<1 for r in ratios),process_ratios=ratios))
q=dict(measurement_complete=True,source_candidate_qualified=qualified,full_objective_achieved=False,processes=2196,observations=count,target_cases=6,control_cases=55,comparisons=sum(r['pairs']for r in comparisons),candidate_peer_wins=sum(r['lin_wins']for r in comparisons),scope='Linux bounded KEEP_SIZE vs transaction-corrected baseline; Full durable mutations, rollback, native writes, inserts, reads; SQLite FULL DELETE/WAL and read DuckDB; other peers unqualified')
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'peer-comparisons.json').write_text(json.dumps(comparisons,indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(q,indent=2)+'\n')
lines=['# Bounded Linux WAL reservation: full-engine paired gate','','All 61 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.','','| Case | Series | Target | Gain % | A/A % | Positive | Pass |','|---|---|---|---:|---:|---:|---|']
for c in cases:
 for series,r in summary[c].items():lines.append(f"| {c} | {series} | {r['target']} | {r['gain_percent']:.3f} | {r['aa_absolute_percent']:.3f} | {r['positive_pairs']}/6 | {r['passes']} |")
lines+=['','| Lin case | Peer | Series | Lin/peer | Lin wins |','|---|---|---|---:|---:|']
for r in comparisons:lines.append(f"| {r['case']} | {r['peer']} | {r['series']} | {r['median_ratio']:.4f} | {r['lin_wins']}/6 |")
lines+=['',f"Source gate: {qualified}. Numeric peer wins: {q['candidate_peer_wins']}/{q['comparisons']}. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory."]
(p/'report.md').write_text('\n'.join(lines)+'\n');print('complete61-case gate:',qualified)
