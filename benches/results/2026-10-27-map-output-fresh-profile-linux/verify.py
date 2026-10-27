from pathlib import Path
import json,statistics as st,hashlib
p=Path(__file__).resolve().parent;s=json.loads((p/'summary.json').read_text());names=['positions','update','delete','patch_fks','index_insert','fts_sync','row_maps','store_delete','move_tail','index_remove_row','swap_columns','drop_keys','fts_move','index_move','quotas','program','prepared','record_row','undo_capture','patch_apply','metadata','updated_row_clone','key_of','insert_key','cas_validate']
meta=json.loads((p/'linux-input-provenance.json').read_text());assert meta['diagnostic_only'];assert all(hashlib.sha256((p/f'map-source-{n}.rs').read_bytes()).hexdigest()==h for n,h in meta['candidate_sha256'].items())
status=json.loads((p/'status.json').read_text());assert len(status)==6 and {r['run']for r in status}=={f'{i:02}-{k}'for i in range(1,4)for k in ['update','delete']} and all(r['exit_code']==0 for r in status)
phase_count=wall_count=0
for kind in ['update','delete']:
 processes=[]
 for i in range(1,4):
  name=f'{i:02}-{kind}';raw=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert raw==json.loads((p/f'{name}-phases.json').read_text());assert len(raw)==33;phase_count+=len(raw)
  data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24
  expected={f'compare/{kind}_1row_{label}/lin'for label in ['1k','10k','100k']};assert {o['case']for o in data['observations']}==expected
  for case in expected:
   values=[o for o in data['observations']if o['case']==case];assert len(values)==8 and all(o['metric']=='wall'and o['operations']==1 and float(o['value'])>0 for o in values)
  wall_count+=24
  assert all(r['kind']==kind and r['diagnostic_only']and len(r['stages_ns'])==25 and all(isinstance(v,int)and v>=0 for v in r['stages_ns'])and r['prepared_ns']>0 and r['stages_ns'][15]>0 for r in raw)
  if kind=='update':assert all(r['stages_ns'][21]>0 for r in raw)
  processes.append(raw)
 for n in [1000,10000,100000]:
  groups=[[r for r in rows if r['rows']==n]for rows in processes];assert all(len(g)==11 for g in groups);reported=s['cases'][f'{kind}/{n}']
  for name in ['prepared_ns','handle_drop_ns']:assert reported[name]==st.median(st.median(r[name]for r in group)for group in groups)
  for idx,name in enumerate(names):assert reported['inclusive_stages_ns'][name]==st.median(st.median(r['stages_ns'][idx]for r in group)for group in groups)
assert phase_count==s['phase_records']==198 and wall_count==s['wall_observations']==144
print(json.dumps(dict(processes=6,phase_records=phase_count,wall_observations=wall_count,all_phase_medians_verified=True,candidate_hashes_verified=True,diagnostic_only=True,full_objective_achieved=False),indent=2))
