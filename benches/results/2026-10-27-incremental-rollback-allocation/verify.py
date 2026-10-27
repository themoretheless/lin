from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
h=lambda b:hashlib.sha256(b).hexdigest()
m=load('provenance.json');status=load('status.json');rows=load('records.json');assert m['source_restored']and not m['performance_qualified']
for name,digest in m['source_sha256'].items():assert h(subprocess.check_output(['git','show',m['source_commit']+':'+name]))==digest,name
assert h((p/'gate.rs').read_bytes())==m['gate_sha256']
for role,binary in m['binaries'].items():
 for name,digest in binary['candidate_sources_sha256'].items():
  assert (m['source_sha256'][name]if role=='baseline'else h((p.parent/'2026-10-27-incremental-owned-rollback'/('candidate-'+Path(name).name)).read_bytes()))==digest
assert len(status)==6 and len({r['pid']for r in status})==6 and {(r['repeat'],r['role'])for r in status}=={(i,role)for i in range(1,4)for role in ['baseline','candidate']}
for r in status:
 assert r['exit_code']==0 and r['binary_sha256']==m['binaries'][r['role']]['sha256']
 raw=[dict(repeat=r['repeat'],role=r['role'],**json.loads(line))for line in(p/f"{r['repeat']:02}-{r['role']}.jsonl").read_text().splitlines()]
 assert raw==[x for x in rows if x['repeat']==r['repeat']and x['role']==r['role']]
index={(r['repeat'],r['role'],r['rows'],r['sample'],r['mode']):r for r in rows}
assert len(rows)==len(index)==216 and set(index)=={(i,role,n,s,mode)for i in range(1,4)for role in ['baseline','candidate']for n in [1000,10000,100000]for s in range(6)for mode in ['update_pair','rollback']}
for r in rows:
 assert r['exact_readback']is True and r['operations']==(1 if r['mode']=='rollback'else 2)
 assert set(r['counters'])=={'allocations','allocated_bytes','deallocations','deallocated_bytes','failed_allocations','reallocations','reallocated_bytes'}
 assert all(type(v)is int and v>=0 for v in r['counters'].values())and r['counters']['failed_allocations']==0
results=[]
for size in [1000,10000,100000]:
 for mode in ['update_pair','rollback']:
  pairs=[]
  for i in range(1,4):
   for sample in range(6):
    a=index[i,'baseline',size,sample,mode]['counters'];b=index[i,'candidate',size,sample,mode]['counters'];pairs.append(dict(repeat=i,sample=sample,allocations_saved=a['allocations']-b['allocations'],allocated_bytes_saved=a['allocated_bytes']-b['allocated_bytes']))
    if mode=='update_pair':assert a==b
    else:assert a['allocations']>b['allocations']and a['allocated_bytes']>b['allocated_bytes']
  ranges={}
  for role in ['baseline','candidate']:
   values=[index[i,role,size,sample,mode]['counters']for i in range(1,4)for sample in range(6)];ranges[role]={key:[min(v[key]for v in values),max(v[key]for v in values)]for key in values[0]}
  results.append(dict(rows=size,mode=mode,ranges=ranges,pairs=pairs))
q=dict(allocation_audit_passed=True,processes=6,windows=216,latency_qualified=False,peak_rss_qualified=False,full_objective_achieved=False,results=results);(p/'summary.json').write_text(json.dumps(q,indent=2)+'\n');print('PASS:6processes/216owner-thread allocation windows;exact UPDATE control counters;all54rollback pairs reduce allocation requests and bytes;no latency/RSS acceptance')
