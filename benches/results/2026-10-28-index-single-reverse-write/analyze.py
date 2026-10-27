from pathlib import Path
import json,math,statistics as st
p=Path(__file__).resolve().parent
records=json.loads((p/'process-status.json').read_text());assert len(records)==36 and {(r['round'],r['mode'])for r in records}=={(i,m)for i in range(1,13)for m in ['baseline','control','candidate']}
med={};count=0
for record in records:
 assert record['exit_code']==0
 rows=[json.loads(line)for line in(p/f"{record['round']:02}-{record['mode']}.jsonl").read_text().splitlines()]
 assert len(rows)==64 and {(r['width'],r['changed'],r['sample'])for r in rows}=={(w,c,s)for w in [1,2,3,8]for c in [False,True]for s in range(8)}
 for row in rows:assert row['operations']==20000 and row['complete_forward_reverse_oracle'] is True and math.isfinite(row['ns_per_update'])and row['ns_per_update']>0
 for width in [1,2,3,8]:
  for changed in [False,True]:med[record['round'],record['mode'],width,changed]=st.median(r['ns_per_update']for r in rows if r['width']==width and r['changed']==changed)
 count+=len(rows)
summary={};passed=True
for width in [1,2,3,8]:
 for changed in [False,True]:
  case=f'width{width}/'+('changed'if changed else'unchanged');summary[case]={}
  for series,start in [('series1',1),('series2',7)]:
   pairs=[]
   for n in range(start,start+6):
    a,b,c=[med[n,m,width,changed]for m in ['baseline','control','candidate']]
    pairs.append(dict(round=n,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
   gain=st.median(r['gain_percent']for r in pairs);noise=st.median(abs(r['aa_percent'])for r in pairs);positive=sum(r['gain_percent']>0 for r in pairs)
   ok=(positive==6 and gain>noise)if changed else gain>=-max(2,noise);passed &=ok
   summary[case][series]=dict(target=changed,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
assert count==2304
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(p/'qualification.json').write_text(json.dumps(dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=36,observations=count),indent=2)+'\n')
lines=['# Single reverse-map write: paired warm index CPU diagnostic','','Previous lazy COW vs redundant reverse-write removal, macOS ARM64. Pure insert_at calls; exhaustive forward/reverse membership checks outside timing. No engine/WAL/RSS or peer-superiority claim.','','|Case|Series|Gain %|Absolute A/A %|Positive|Pass|','|---|---|---:|---:|---:|---|']
for case,series_rows in summary.items():
 for series,r in series_rows.items():lines.append(f"|{case}|{series}|{r['gain_percent']:.3f}|{r['aa_absolute_percent']:.3f}|{r['positive_pairs']}/6|{r['passes']}|")
lines+=['',f'Component gate: {passed}. All 36 processes / 2304 observations retained.']
(p/'report.md').write_text('\n'.join(lines)+'\n');print('component gate:',passed)
