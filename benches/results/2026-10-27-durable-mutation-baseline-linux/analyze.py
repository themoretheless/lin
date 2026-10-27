from pathlib import Path
import json,math,re,statistics as st,sys
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
cases=load('cases.json');records=load('process-status.json');assert len(cases)==len(set(cases))==24
assert len(records)==576 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,m,c)for i in range(1,13)for m in ['baseline','control']for c in cases}
med={};rss={};observations=0
for r in records:
 assert r['exit_code']==0
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}';d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
 values=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);observations+=1
 k=r['round'],r['mode'],r['case'];med[k]=st.median(values)
 value=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert value==r['peak_rss_kib']>0;rss[k]=value
assert observations==4608
noise={};comparisons=[]
for case in cases:
 noise[case]={}
 for label,start in [('series1',1),('series2',7)]:
  noise[case][label]=dict(aa_absolute_percent=st.median(abs(100*(1-med[i,'control',case]/med[i,'baseline',case]))for i in range(start,start+6)),peak_rss_kib={m:st.median(rss[i,m,case]for i in range(start,start+6))for m in ['baseline','control']})
for case in cases:
 if not case.endswith('/lin'):continue
 for peer in ['sqlite_delete','sqlite_wal']:
  sql=case[:-3]+peer
  for mode in ['baseline','control']:
   for label,start in [('series1',1),('series2',7)]:
    rows=[dict(round=i,lin_ns=med[i,mode,case],sqlite_ns=med[i,mode,sql],lin_over_sqlite=med[i,mode,case]/med[i,mode,sql])for i in range(start,start+6)]
    comparisons.append(dict(case=case,peer=peer,mode=mode,series=label,pairs=6,lin_ns=st.median(r['lin_ns']for r in rows),sqlite_ns=st.median(r['sqlite_ns']for r in rows),median_ratio=st.median(r['lin_over_sqlite']for r in rows),lin_wins=sum(r['lin_over_sqlite']<1 for r in rows),lin_aa_percent=noise[case][label]['aa_absolute_percent'],sqlite_aa_percent=noise[sql][label]['aa_absolute_percent'],process_pairs=rows))
q=dict(measurement_complete=True,processes=576,observations=observations,comparisons=384,lin_wins=sum(r['lin_wins']for r in comparisons),full_objective_achieved=False,source_candidate_qualified=False,scope='production-source full-sync mutation baseline only; no optimization candidate; both SQLite FULL DELETE/WAL retained')
(p/'summary.json').write_text(json.dumps(dict(cases=noise,comparisons=comparisons),indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(q,indent=2)+'\n')
lines=['# Full-sync mutation baseline vs SQLite','','Source qualification of an optimization is not claimed. Clean-reopen readback is not crash proof. Both SQLite FULL DELETE and WAL results retained. Ratio <1 means Lin faster. Each row contains six within-round peer comparisons; baseline/control are independent runs of the same executable. All process losses retained.','','| Case | Peer | Role | Series | Lin ns | SQLite ns | Lin/SQLite | Lin wins | Lin A/A % | SQLite A/A % |','|---|---|---|---|---:|---:|---:|---:|---:|---:|']
for r in comparisons:lines.append(f"| {r['case']} | {r['peer']} | {r['mode']} | {r['series']} | {r['lin_ns']:.2f} | {r['sqlite_ns']:.2f} | {r['median_ratio']:.4f} | {r['lin_wins']}/6 | {r['lin_aa_percent']:.3f} | {r['sqlite_aa_percent']:.3f} |")
lines+=['',f"Numeric Lin wins: {q['lin_wins']}/384. Full eight-peer objective remains unachieved.",'','## Whole-process peak RSS (KiB)','','This includes setup, oracle and reopen; it is not isolated mutation memory.','','| Case | Series | Baseline | Control |','|---|---|---:|---:|']
for c in cases:
 for series,r in noise[c].items():lines.append(f"| {c} | {series} | {r['peak_rss_kib']['baseline']} | {r['peak_rss_kib']['control']} |")
(p/'report.md').write_text('\n'.join(lines)+'\n');print('Complete baseline matrix:',q['lin_wins'],'/384 numeric peer wins')
