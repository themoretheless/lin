from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;raw=p.parent/'2026-10-27-cow-row-linux'
s=json.loads((raw/'summary.json').read_text());status={r['run']:r for r in json.loads((raw/'status.json').read_text())};losses=[]
for case in ['compare/update_1row_1k/lin','compare/update_1row_10k/lin','compare/update_1row_100k/lin']:
 for series,group in s[case].items():
  for row in group['processes']:
   if row['gain_percent']<=0:
    i=row['round'];entry=dict(case=case,series=series,**row)
    entry['load_before']={mode:status[f'{i:02}-{mode}']['load_before'] for mode in ['baseline','control','candidate']}
    losses.append(entry)
assert len(losses)==5
out={'failed_target_pairs':losses,'load_before_1m_min':min(v['load_before'][0]for v in status.values()),'load_before_1m_max':max(v['load_before'][0]for v in status.values()),'excluded_pairs':0,'overloaded_runner_cause_established':False,'input_sha256':{n:hashlib.sha256((raw/n).read_bytes()).hexdigest()for n in ['status.json','summary.json','qualification.json']}}
(p/'audit.json').write_text(json.dumps(out,indent=2)+'\n')
print('Five target losses retained; no pairs excluded; load alone does not explain failures')
