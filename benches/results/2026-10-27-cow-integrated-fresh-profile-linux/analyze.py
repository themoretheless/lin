from pathlib import Path
import json,statistics as st
p=Path(__file__).resolve().parent;names=['positions','update','delete','patch_fks','index_insert','fts_sync','row_maps','store_delete','move_tail','index_remove_row','swap_columns','drop_keys','fts_move','index_move','quotas','program','prepared','record_row','undo_capture','patch_apply','metadata','pack_clone','key_of','insert_key','cas_validate'];status=json.loads((p/'status.json').read_text());assert len(status)==6 and all(r['exit_code']==0 for r in status);out={};records_total=0;observations=0
for kind in ['update','delete']:
 groups=[]
 for i in range(1,4):
  name=f'{i:02}-{kind}';records=json.loads((p/f'{name}-phases.json').read_text());raw=[json.loads(s[6:])for s in(p/f'{name}.log').read_text().splitlines()if s.startswith('PHASE ')];assert records==raw;records_total+=len(raw)
  d=json.loads((p/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==3 and len(d['observations'])==24;observations+=24
  assert all(r['diagnostic_only']and r['kind']==kind and len(r['stages_ns'])==25 and r['prepared_ns']>0 and r['stages_ns'][1 if kind=='update' else 2]>0 for r in raw)
  groups.append(records)
 for n in [1000,10000,100000]:
  gs=[[r for r in g if r['rows']==n]for g in groups];assert all(len(g)>=8 for g in gs)
  entry={'processes':3,'records_per_process':[len(g)for g in gs],'prepared_ns':st.median(st.median(r['prepared_ns']for r in g)for g in gs),'handle_drop_ns':st.median(st.median(r['handle_drop_ns']for r in g)for g in gs),'inclusive_stages_ns':{name:st.median(st.median(r['stages_ns'][j]for r in g)for g in gs)for j,name in enumerate(names)}}
  if kind=='update':entry['update_unattributed_ns']=st.median(st.median(r['stages_ns'][1]-sum(r['stages_ns'][j]for j in [0,3,4,5,6,17,18,19,20,24])for r in g)for g in gs)
  out[f'{kind}/{n}']=entry
assert observations==144
(p/'summary.json').write_text(json.dumps({'diagnostic_only':True,'phase_records':records_total,'wall_observations':observations,'cases':out},indent=2)+'\n');print('diagnostic records',records_total,'wall observations',observations)
