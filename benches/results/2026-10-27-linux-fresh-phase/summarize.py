from pathlib import Path
import json,statistics as st,hashlib
p=Path(__file__).resolve().parent
names=['positions','update','delete','fks','index_insert','fts_sync','row_maps_sync','store_delete','move_tail','index_remove','swap_columns','drop_keys','fts_move','index_move','quotas','program','prepared','before_clone','updated_clone','undo_capture','store_replace','pack_clone']
status=json.loads((p/'status.json').read_text());assert len(status)==6 and all(x['exit_code']==0 for x in status)
out={};total=0
for mode in ['baseline','candidate']:
 for n in [1000,10000,100000]:
  med=[]
  for i in range(1,4):
   records=json.loads((p/f'{i:02}-{mode}-phases.json').read_text());rows=[r for r in records if r['rows']==n];assert len(rows)>=8 and all(len(r['stages_ns'])==22 and r['diagnostic_only']for r in rows)
   total+=len(rows)
   q={name:st.median(r['stages_ns'][j]for r in rows)for j,name in enumerate(names)}
   q.update(process=i,records=len(rows),call_ns=st.median(r['prepared_ns']for r in rows),handle_drop_ns=st.median(r['handle_drop_ns']for r in rows))
   q['update_unattributed_ns']=st.median(r['stages_ns'][1]-sum(r['stages_ns'][j]for j in [0,3,4,5,6,17,18,19,20])for r in rows)
   med.append(q)
  out[f'{mode}/{n}']={'median_process_stages':{k:st.median(q[k]for q in med)for k in med[0]if k not in ['process','records']},'processes':med}
(p/'summary.json').write_text(json.dumps({'total_phase_records':total,'all_records_including_framework_calls':True,'acceptance_timing':False,'results':out},indent=2)+'\n')
print('records',total)
for k,x in out.items():print(k,{n:x['median_process_stages'][n]for n in ['call_ns','before_clone','update_unattributed_ns']})
