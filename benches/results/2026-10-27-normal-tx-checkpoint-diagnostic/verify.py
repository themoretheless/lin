from pathlib import Path
import json,hashlib,subprocess,sys
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
def middle(values):
 ordered=sorted(values);n=len(ordered);return(ordered[(n-1)//2]+ordered[n//2])/2
m=load('provenance.json');records=load('process-status.json');summary=load('summary.json');env=load('environment.json')
assert m['source_restored']and not m['production_source_changed']and not m['optimization_candidate']
assert hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest()==m['probe_sha256']
for name,digest in dict(m['source_sha256'],**m['build_inputs_sha256']).items():
 source=subprocess.check_output(['git','show',m['source_commit']+':'+name]);assert hashlib.sha256(source).hexdigest()==digest,name
assert len(records)==6 and len({r['pid']for r in records})==6
assert [(r['repeat'],r['engine'])for r in records]==[(1,'lin'),(1,'sqlite'),(2,'sqlite'),(2,'lin'),(3,'lin'),(3,'sqlite')]
assert env['diagnostic_only']and not env['policy_changed']and not env['paired_candidate_qualification']
expected=[]
for row in records:
 assert row['exit_code']==0 and row['binary_sha256']==m['binary']['sha256']
 d=load(row['output']);assert d['engine']==row['engine']and d['transactions']==1000 and d['operations_per_transaction']==5 and d['full_values_after_reopen']
 samples=d['samples'];assert len(samples)==1000;assert all(type(v['elapsed_ns'])is int and v['elapsed_ns']>0 for v in samples)
 values=[v['elapsed_ns']for v in samples];item=dict(repeat=row['repeat'],engine=row['engine'],transactions=1000,total_ns=sum(values),mean_ns=sum(values)/1000,median_ns=middle(values))
 for index,v in enumerate(samples):assert v['transaction']==index
 if row['engine']=='lin':
  cp=[];ordinary=[]
  for index,v in enumerate(samples):
   before=(index+1)%32;after=(index+2)%32
   assert type(v['checkpoint'])is bool and v['writes_before']==before and v['writes_after']==after and v['checkpoint']==(after==0)
   (cp if after==0 else ordinary).append(v['elapsed_ns'])
  assert len(cp)==31 and len(ordinary)==969
  item.update(checkpoints=len(cp),regular_transactions=len(ordinary),checkpoint_mean_ns=sum(cp)/len(cp),regular_mean_ns=sum(ordinary)/len(ordinary),checkpoint_total_percent=100*sum(cp)/sum(values))
 else:
  assert d['sqlite_settings']==dict(journal='wal',synchronous=1,wal_autocheckpoint_pages=1000)
  assert all(v['checkpoint']is None and v['writes_before']is None and v['writes_after']is None for v in samples)
 expected.append(item)
assert summary==dict(diagnostic_only=True,optimization_candidate=False,source_candidate_qualified=False,full_objective_achieved=False,processes=6,transactions=6000,runs=expected)
print('Independent diagnostic audit PASS: six fresh processes,6000 ordered observations,93 checkpoint transactions,total shares and exact source hashes; no source qualification')
