from pathlib import Path
import hashlib,json,subprocess,sys,difflib
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());m=load('provenance.json');records=load('process-status.json');summary=load('summary.json');env=load('environment.json')
def middle(values):
 v=sorted(values);return(v[(len(v)-1)//2]+v[len(v)//2])/2
assert m['source_restored']and not m['production_change']and not m['optimization_candidate']
assert env['diagnostic_only']and not env['policy_changed']and env['clock_observer_controls']
assert hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest()==m['probe_sha256']and hashlib.sha256((p/'instrumented-store.rs').read_bytes()).hexdigest()==m['instrumented_store_sha256']
original_store=None
for name,digest in dict(m['source_sha256'],**m['build_inputs_sha256']).items():
 source=subprocess.check_output(['git','show',m['source_commit']+':'+name]);assert hashlib.sha256(source).hexdigest()==digest,name
 if name=='src/store.rs':original_store=source.decode()
diff=list(difflib.unified_diff(original_store.splitlines(),(p/'instrumented-store.rs').read_text().splitlines()))
assert not any(line.startswith('-')and not line.startswith('---')for line in diff),'original checkpoint steps removed'
assert len(records)==15 and len({r['pid']for r in records})==15
assert {(r['repeat'],r['role'])for r in records}=={(i,role)for i in range(1,4)for role in ['original','control','disabled','traced','sqlite']}
names=['fence','wal_sync','prepare','snapshot','head','fts','compact'];rows=[];phases=[]
for record in records:
 role=record['role'];engine='sqlite'if role=='sqlite'else'lin';assert record['engine']==engine and record['exit_code']==0 and record['trace_enabled']==(role=='traced')
 assert record['binary_sha256']==m['binaries']['instrumented'if role in ['traced','disabled']else'original']['sha256']
 d=load(record['output']);samples=d['samples'];assert d['engine']==engine and d['transactions']==1000 and d['operations_per_transaction']==5 and d['full_values_after_reopen']and len(samples)==1000
 for i,s in enumerate(samples):
  assert s['transaction']==i and type(s['elapsed_ns'])is int and s['elapsed_ns']>0
  if engine=='lin':assert type(s['checkpoint'])is bool and s['writes_before']==(i+1)%32 and s['writes_after']==(i+2)%32 and s['checkpoint']==((i+2)%32==0)
  else:assert s['checkpoint']is None and s['writes_before']is None and s['writes_after']is None
 if engine=='sqlite':assert d['sqlite_settings']==dict(journal='wal',synchronous=1,wal_autocheckpoint_pages=1000)
 values=[s['elapsed_ns']for s in samples];rows.append(dict(repeat=record['repeat'],role=role,mean_ns=sum(values)/1000,median_ns=middle(values)))
 text=(p/record['trace']).read_text();events=[]
 for line in text.splitlines():
  if line.startswith('LIN_CHECKPOINT_STAGE '):events.append(json.loads(line.partition(' ')[2]))
 if role!='traced':assert not events;continue
 for event in events:
  ns=event['stage_ns'];assert event['phase_names']==names and len(ns)==7 and all(type(v)is int and v>=0 for v in ns)and type(event['whole_ns'])is int and event['whole_ns']>0 and sum(ns)<=event['whole_ns']
 auto=[event for event in events if event['generation']in range(32,993,32)];assert[event['generation']for event in auto]==list(range(32,993,32))
 assert all(e['rows']==1+5*(e['generation']-1)for e in auto)
 phases.append(dict(repeat=record['repeat'],automatic_frames=31,other_frames=len(events)-31,whole_ns=middle(e['whole_ns']for e in auto),stage_ns={name:middle(e['stage_ns'][i]for e in auto)for i,name in enumerate(names)},stage_percent={name:middle(100*e['stage_ns'][i]/e['whole_ns']for e in auto)for i,name in enumerate(names)}))
observer=[]
for repeat in range(1,4):
 means={r['role']:r['mean_ns']for r in rows if r['repeat']==repeat}
 observer.append(dict(repeat=repeat,control_over_original=means['control']/means['original'],disabled_over_original=means['disabled']/means['original'],traced_over_disabled=means['traced']/means['disabled'],original_over_sqlite=means['original']/means['sqlite']))
assert summary==dict(diagnostic_only=True,source_candidate_qualified=False,full_objective_achieved=False,processes=15,transaction_observations=15000,automatic_checkpoint_frames=93,runs=rows,phases=phases,observer=observer)
print('Independent audit PASS:15000 transaction observations,93 auto checkpoint frames,stage medians/fractions and observer ratios;exact historical sources;no checkpoint statements removed')
