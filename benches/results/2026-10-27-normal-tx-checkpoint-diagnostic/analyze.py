from pathlib import Path
import json,statistics as st,hashlib
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());m=load('provenance.json');records=load('process-status.json');assert len(records)==6 and {(r['repeat'],r['engine'])for r in records}=={(i,e)for i in range(1,4)for e in ['lin','sqlite']};out=[]
assert m['source_restored']and all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==v for f,v in m['source_sha256'].items())
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==m['binary']['sha256'];d=load(r['output']);assert d['engine']==r['engine']and d['transactions']==1000 and d['operations_per_transaction']==5 and d['full_values_after_reopen'];samples=d['samples'];assert len(samples)==1000
 assert [v['transaction']for v in samples]==list(range(1000))and all(isinstance(v['elapsed_ns'],int)and v['elapsed_ns']>0 for v in samples)
 item=dict(repeat=r['repeat'],engine=r['engine'],transactions=1000,total_ns=sum(v['elapsed_ns']for v in samples),mean_ns=st.mean(v['elapsed_ns']for v in samples),median_ns=st.median(v['elapsed_ns']for v in samples))
 if r['engine']=='lin':
  expected=1
  for index,s in enumerate(samples):
   assert s['writes_before']==expected;expected=(expected+1)%32;assert s['writes_after']==expected and s['checkpoint']==(expected==0)
  cp=[v['elapsed_ns']for v in samples if v['checkpoint']];regular=[v['elapsed_ns']for v in samples if not v['checkpoint']];assert len(cp)==31 and len(regular)==969
  item.update(checkpoints=31,regular_transactions=969,checkpoint_mean_ns=st.mean(cp),regular_mean_ns=st.mean(regular),checkpoint_total_percent=100*sum(cp)/item['total_ns'])
 else:
  assert d['sqlite_settings']==dict(journal='wal',synchronous=1,wal_autocheckpoint_pages=1000)
  assert all(v['checkpoint']is None and v['writes_before']is None and v['writes_after']is None for v in samples)
 out.append(item)
(p/'summary.json').write_text(json.dumps(dict(diagnostic_only=True,optimization_candidate=False,source_candidate_qualified=False,full_objective_achieved=False,processes=6,transactions=6000,runs=out),indent=2)+'\n')
lines=['# Normal-mode five-write transaction diagnostic','','Three fresh processes per engine,1000 growing transactions with five inserts each. Dynamic Lin run and SQLite execute parse inside timing, following the old test. Per-transaction clocks and Lin counter reads outside timing make this a new diagnostic contract, not a replication of its aggregate timer. Default Lin checkpoint every32 commits and SQLite WAL autocheckpoint1000 pages are unchanged. All supplied values and multiplicity checked after ordinary close/reopen.','','| Repeat | Engine | Mean ns/tx | Median ns/tx | Checkpoints | Checkpoint mean ns | Regular mean ns | Checkpoint share of timed total % |','|---:|---|---:|---:|---:|---:|---:|---:|']
for r in out:lines.append(f"| {r['repeat']} | {r['engine']} | {r['mean_ns']:.1f} | {r['median_ns']:.1f} | {r.get('checkpoints','not observed')} | {r.get('checkpoint_mean_ns',0):.1f} | {r.get('regular_mean_ns',0):.1f} | {r.get('checkpoint_total_percent',0):.2f} |")
lines+=['','Checkpoint-labelled samples include the whole transaction, not isolated checkpoint spans. SQLite checkpoints are not independently instrumented. Counter reads and clock overhead are not calibrated away. No candidate/A/A causal speed claim, no Linux inference from macOS and no durability-policy recommendation follows from this diagnostic. End-of-run Normal close/checkpoint is outside the operation timer and occurs before owned fixture cleanup. Normal mode does not promise each transaction survives power loss.','',f"Frozen source: `{m['source_commit']}`; current production Row alias preserved. Raw six process outputs, source/lock/binary hashes and build log retained."]
(p/'report.md').write_text('\n'.join(lines)+'\n')
for r in out:
 if r['engine']=='lin':print('repeat',r['repeat'],'checkpoint share',round(r['checkpoint_total_percent'],2),'mean',round(r['mean_ns'],1),'ordinary',round(r['regular_mean_ns'],1))
