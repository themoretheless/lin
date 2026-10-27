from pathlib import Path
import json,math,re,statistics as st,sys
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
roles=['original','control','disabled','traced'];names=['exec_program','quota','encode','crc','write','sync','checkpoint','rollback'];cases=load('cases.json');records=load('process-status.json');assert len(cases)==len(set(cases))==24
assert len(records)==576 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,role,c)for i in range(1,7)for role in roles for c in cases}
med={};rss={};trace_rows={c:[]for c in cases if c.endswith('/lin')};observations=0;frame_count=0
for r in records:
 assert r['exit_code']==0
 c,role,i=r['case'],r['mode'],r['round'];name=c.replace('/','_');base=p/f'{i:02}-{role}';d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['observations'])==8 and len(d['cases'])==1 and d['cases'][0]['id']==c
 vals=[]
 for o in d['observations']:
  assert o['case']==c and o['operations']==1 and o['metric']=='wall';v=float(o['value']);assert math.isfinite(v)and v>0;vals.append(v);observations+=1
 med[i,role,c]=st.median(vals);peak=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert peak==r['peak_rss_kib']>0;rss[i,role,c]=peak
 path=base/(name+'.phases.json')
 if role!='traced':assert not path.exists();continue
 t=json.loads(path.read_text());assert t['phase_names']==names and t['timing_qualified']is False
 frames=t['records'];frame_count+=len(frames)
 if not c.endswith('/lin'):assert not frames;continue
 assert frames;rollback='rollback_'in c
 for f in frames:
  assert f['wall_ns']>0 and len(f['phase_ns'])==len(f['phase_calls'])==8 and sum(f['phase_ns'])<=f['wall_ns']
  assert f['phase_calls'][0]==1 and f['phase_calls'][6]==0 and f['phase_calls'][7]==int(rollback)
  assert all(f['phase_calls'][k]==int(not rollback)for k in [2,3,4,5])and (f['payload_bytes']==0)==rollback
 trace_rows[c].append(dict(round=i,frames=len(frames),whole_ns=st.median(f['wall_ns']for f in frames),payload_bytes=st.median(f['payload_bytes']for f in frames),phases={n:dict(ns=st.median(f['phase_ns'][k]for f in frames),share_percent=st.median(100*f['phase_ns'][k]/f['wall_ns']for f in frames),calls=sorted(set(f['phase_calls'][k]for f in frames)))for k,n in enumerate(names)}))
assert observations==4608
wall={};peer=[]
for c in cases:
 aa=st.median(abs(100*(1-med[i,'control',c]/med[i,'original',c]))for i in range(1,7))
 ratios={role:st.median(med[i,role,c]/med[i,'original',c]for i in range(1,7))for role in roles}
 wall[c]=dict(aa_absolute_percent=aa,median_ns={role:st.median(med[i,role,c]for i in range(1,7))for role in roles},ratio_to_original=ratios,traced_over_disabled=st.median(med[i,'traced',c]/med[i,'disabled',c]for i in range(1,7)),peak_rss_kib={role:st.median(rss[i,role,c]for i in range(1,7))for role in roles})
 if c.endswith('/lin'):
  for sql in ['sqlite_delete','sqlite_wal']:
   for role in roles:
    ratios=[med[i,role,c]/med[i,role,c[:-3]+sql]for i in range(1,7)];peer.append(dict(case=c,peer=sql,mode=role,pairs=6,median_ratio=st.median(ratios),lin_wins=sum(v<1 for v in ratios)))
phases={}
for c,rows in trace_rows.items():
 assert len(rows)==6 and {r['round']for r in rows}==set(range(1,7))
 phases[c]=dict(frames=sum(r['frames']for r in rows),whole_ns=st.median(r['whole_ns']for r in rows),payload_bytes=st.median(r['payload_bytes']for r in rows),phases={n:dict(ns=st.median(r['phases'][n]['ns']for r in rows),share_percent=st.median(r['phases'][n]['share_percent']for r in rows))for n in names},processes=rows)
q=dict(diagnostic_complete=True,processes=576,observations=observations,trace_frames=frame_count,uninstrumented_peer_comparisons=192,uninstrumented_peer_wins=sum(r['lin_wins']for r in peer if r['mode']in ['original','control']),source_candidate_qualified=False,full_objective_achieved=False,scope='six-round caller-thread diagnostic, no optimization candidate or all-peer acceptance; all invocation phases separate from measured wall observations')
(p/'summary.json').write_text(json.dumps(dict(wall=wall,phases=phases,peers=peer),indent=2)+'\n');(p/'qualification.json').write_text(json.dumps(q,indent=2)+'\n')
lines=['# Durable mutation caller-thread phase diagnostic','','Original/control use the same uninstrumented production-source binary. Disabled/traced use the same instrumented binary. A/A is median absolute within-round difference. Ratios >1 mean slower. This is diagnostic evidence, not acceptance of an optimization.','','## Measured benchmark wall observations','','| Case | Original ns | Original A/A % | Disabled/original | Traced/original | Traced/disabled |','|---|---:|---:|---:|---:|---:|']
for c,s in wall.items():lines.append(f"| {c} | {s['median_ns']['original']:.2f} | {s['aa_absolute_percent']:.3f} | {s['ratio_to_original']['disabled']:.4f} | {s['ratio_to_original']['traced']:.4f} | {s['traced_over_disabled']:.4f} |")
lines+=['','## All-invocation traced phases','','These records include fresh validation/calibration work as well as measurement. They are not paired with individual benchmark observations. Each aggregate is the median of six process-level medians. Share uses each invocation phase/whole-work time, then medians within/across processes. Checkpoint is an inclusive parent and inactive in these fixtures; never add active checkpoint to its nested phases.','','| Case | Records | Whole-work ns | Payload bytes | Exec % | Encode % | CRC % | Write % | Sync % | Rollback % |','|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
for c,s in phases.items():lines.append(f"| {c} | {s['frames']} | {s['whole_ns']:.2f} | {s['payload_bytes']} | "+' | '.join(f"{s['phases'][n]['share_percent']:.3f}"for n in ['exec_program','encode','crc','write','sync','rollback'])+' |')
lines+=['','## Peer numeric comparisons','','Both SQLite FULL DELETE/WAL retained; instrumented labels do not qualify the original binary.','','| Case | Peer | Role | Lin/SQLite | Lin wins/6 |','|---|---|---|---:|---:|']
for r in peer:lines.append(f"| {r['case']} | {r['peer']} | {r['mode']} | {r['median_ratio']:.4f} | {r['lin_wins']} |")
lines+=['','## Whole-process peak RSS (KiB)','','This includes fixture/oracle/reopen and buffered trace data; it is not isolated mutation memory.','','| Case | Original | Control | Disabled | Traced |','|---|---:|---:|---:|---:|']
for c,s in wall.items():lines.append(f"| {c} | "+' | '.join(str(s['peak_rss_kib'][role])for role in roles)+' |')
lines+=['',f"Complete:576 processes,4608 measured observations,{frame_count} separately scoped invocation records. Full eight-peer objective remains unachieved."]
(p/'report.md').write_text('\n'.join(lines)+'\n');print('Complete diagnostic:',frame_count,'invocation frames; original peer wins',q['uninstrumented_peer_wins'],'/192')
