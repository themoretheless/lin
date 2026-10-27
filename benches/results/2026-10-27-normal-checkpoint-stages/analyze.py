from pathlib import Path
import json,statistics as st
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());records=load('process-status.json');assert len(records)==15 and {(r['repeat'],r['role'])for r in records}=={(i,m)for i in range(1,4)for m in ['original','control','disabled','traced','sqlite']};rows=[];phases=[];names=['fence','wal_sync','prepare','snapshot','head','fts','compact']
for r in records:
 assert r['exit_code']==0;d=load(r['output']);assert d['engine']==r['engine']and d['transactions']==1000 and d['operations_per_transaction']==5 and d['full_values_after_reopen'];samples=d['samples'];assert len(samples)==1000 and[s['transaction']for s in samples]==list(range(1000))and all(type(s['elapsed_ns'])is int and s['elapsed_ns']>0 for s in samples)
 if r['engine']=='lin':assert all(s['writes_before']==(i+1)%32 and s['writes_after']==(i+2)%32 and s['checkpoint']==((i+2)%32==0)for i,s in enumerate(samples))
 else:assert d['sqlite_settings']==dict(journal='wal',synchronous=1,wal_autocheckpoint_pages=1000)
 rows.append(dict(repeat=r['repeat'],role=r['role'],mean_ns=st.mean(s['elapsed_ns']for s in samples),median_ns=st.median(s['elapsed_ns']for s in samples)))
 text=(p/r['trace']).read_text();events=[json.loads(line.split('LIN_CHECKPOINT_STAGE ',1)[1])for line in text.splitlines()if line.startswith('LIN_CHECKPOINT_STAGE ')]
 if r['role']!='traced':assert not events;continue
 for e in events:
  assert e['phase_names']==names and len(e['stage_ns'])==7 and all(type(n)is int and n>=0 for n in e['stage_ns'])and type(e['whole_ns'])is int and e['whole_ns']>0 and sum(e['stage_ns'])<=e['whole_ns']
 auto=[e for e in events if 0<e['generation']<=992 and e['generation']%32==0];assert[e['generation']for e in auto]==list(range(32,993,32))
 assert all(e['rows']==1+(e['generation']-1)*5 for e in auto)
 phases.append(dict(repeat=r['repeat'],automatic_frames=len(auto),other_frames=len(events)-len(auto),whole_ns=st.median(e['whole_ns']for e in auto),stage_ns={name:st.median(e['stage_ns'][i]for e in auto)for i,name in enumerate(names)},stage_percent={name:st.median(100*e['stage_ns'][i]/e['whole_ns']for e in auto)for i,name in enumerate(names)}))
observer=[]
for repeat in range(1,4):
 m={r['role']:r['mean_ns']for r in rows if r['repeat']==repeat};observer.append(dict(repeat=repeat,control_over_original=m['control']/m['original'],disabled_over_original=m['disabled']/m['original'],traced_over_disabled=m['traced']/m['disabled'],original_over_sqlite=m['original']/m['sqlite']))
q=dict(diagnostic_only=True,source_candidate_qualified=False,full_objective_achieved=False,processes=15,transaction_observations=15000,automatic_checkpoint_frames=93,runs=rows,phases=phases,observer=observer)
(p/'summary.json').write_text(json.dumps(q,indent=2)+'\n')
lines=['# Normal-mode checkpoint stage diagnostic','','Baseline/control use the same original binary; disabled/traced use the same instrumented binary.3 fresh processes per role,1000 growing transactions each,5 writes/transaction. SQLite WAL NORMAL retains its1000-page autocheckpoint; Lin retains32-commit checkpoints and every existing sync call. Full input-value reopen passes in all15 processes. This is a local macOS diagnostic, not optimization qualification.','','| Repeat | Phase | Median ns | Median fraction of checkpoint % |','|---:|---|---:|---:|']
for r in phases:
 for name in names:lines.append(f"| {r['repeat']} | {name} | {r['stage_ns'][name]:.1f} | {r['stage_percent'][name]:.3f} |")
lines+=['','Fractions are computed per automatic checkpoint then medians within each process. Whole-function timing is inclusive; do not add it to nested stages. Snapshot includes encoding,file sync,rename and directory sync. FTS includes all cache writes. Fence includes acquisition; compact includes truncate,seek and sync_all. Those composite spans do not isolate individual syscalls. Final close/reopen checkpoints are preserved in raw logs but excluded from the automatic-checkpoint table.','','| Repeat | Control/original | Disabled/original | Traced/disabled | Original/SQLite |','|---:|---:|---:|---:|---:|']
for r in observer:lines.append(f"| {r['repeat']} | {r['control_over_original']:.4f} | {r['disabled_over_original']:.4f} | {r['traced_over_disabled']:.4f} | {r['original_over_sqlite']:.4f} |")
lines+=['','Observer ratios use whole transaction mean within each fresh process. Counter reads,clock overhead,code layout and trace serialization/stderr inside transaction time are not calibrated away. Whole stage timing stops before trace JSON emission,while the transaction timer includes emission. Ratios from this small three-process diagnostic are descriptive,not a source gate or cross-engine speed claim. All raw losses are retained. Production sources were restored; hashes and both executable identities are retained.']
(p/'report.md').write_text('\n'.join(lines)+'\n')
for name in names:print(name,round(st.median(r['stage_percent'][name]for r in phases),3),'%')
