from pathlib import Path
import json,statistics as st,math
p=Path(__file__).resolve().parent;status=json.loads((p/'status.json').read_text());expected={f'{i:02}-{role}'for i in range(1,13)for role in ['baseline','control','candidate']}|{f'{i:02}-staged'for i in range(1,4)};assert len(status)==39 and {r['run']for r in status}==expected and all(r['exit_code']==0 for r in status)
values={};wall_count=phase_count=0
for name in expected:
 raw=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert raw==json.loads((p/f'{name}-phases.json').read_text());assert len(raw)==33;phase_count+=33
 d=json.loads((p/name/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==24
 assert all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in d['observations']);wall_count+=24
 for n in [1000,10000,100000]:
  rows=[r for r in raw if r['rows']==n];assert len(rows)==11 and all(r['diagnostic_only']and r['kind']=='update'and len(r['stages_ns'])==28 for r in rows)
  values[name,n]={'prepared_ns':st.median(r['prepared_ns']for r in rows),'copy_ns':st.median(r['stages_ns'][21]for r in rows),'keys_ns':st.median(r['stages_ns'][25]for r in rows),'values_ns':st.median(r['stages_ns'][26]for r in rows),'build_ns':st.median(r['stages_ns'][27]for r in rows)}
assert wall_count==936 and phase_count==1287
out={};accepted=True
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns']:
  key=f'{n}/{metric}';out[key]={}
  for label,rounds in [('series1',range(1,7)),('series2',range(7,13))]:
   rows=[]
   for i in rounds:
    a,b,c=(values[f'{i:02}-{role}',n][metric]for role in ['baseline','control','candidate']);rows.append(dict(round=i,baseline=a,control=b,candidate=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
   gain=st.median(r['gain_percent']for r in rows);noise=st.median(abs(r['aa_percent'])for r in rows);positive=sum(r['gain_percent']>0 for r in rows);passed=positive==6 and gain>noise;accepted &= passed
   out[key][label]=dict(gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,processes=rows);print(key,label,round(gain,3),round(noise,3),positive,passed)
staged={str(n):{metric:st.median(values[f'{i:02}-staged',n][metric]for i in range(1,4))for metric in ['prepared_ns','copy_ns','keys_ns','values_ns','build_ns']}for n in [1000,10000,100000]}
(p/'summary.json').write_text(json.dumps(dict(diagnostic_only=True,comparisons=out,staged=staged,phase_records=phase_count,wall_observations=wall_count),indent=2)+'\n');(p/'selection.json').write_text(json.dumps(dict(diagnostic_selection_qualified=accepted,engine_qualified=False,full_objective_achieved=False),indent=2)+'\n')
