from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;allrows={};cases={'default_1k','default_10k','single_field_1k','unicode_1k'}
for i in range(1,13):
 rows=[json.loads(l)for l in(p/f'process-{i}.jsonl').read_text().splitlines()];assert {r['case']for r in rows}==cases and len(rows)==4
 for r in rows:
  assert r['exact_values']and len(r['ns'])==3 and all(len(samples)==12 and all(math.isfinite(v)and v>0 for v in samples)for samples in r['ns'])
  a,b,c=map(st.median,r['ns']);allrows.setdefault(r['case'],[]).append(dict(process=i,baseline_ns=a,control_ns=b,candidate_ns=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
out={};accepted=True
for case,rows in sorted(allrows.items()):
 out[case]={}
 for label,group in [('series1',rows[:6]),('series2',rows[6:])]:
  gain=st.median(r['gain_percent']for r in group);noise=st.median(abs(r['aa_percent'])for r in group);positive=sum(r['gain_percent']>0 for r in group)
  passed=(positive==6 and gain>noise)if case.startswith('default_')else(gain>=-max(2,noise));accepted &= passed
  out[case][label]=dict(gain_percent=gain,aa_absolute_percent=noise,positive_processes=positive,passes=passed,processes=group)
  print(case,label,round(gain,3),round(noise,3),positive,passed)
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(dict(component_qualified=accepted,engine_qualified=False,full_objective_achieved=False,processes=12,observations=1728),indent=2)+'\n')
