from pathlib import Path
import json,hashlib,subprocess,statistics
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());h=lambda b:hashlib.sha256(b).hexdigest();m=load('provenance.json');status=load('status.json');assert m['source_restored'] and h((p/'gate.rs').read_bytes())==m['gate_sha256']
for n,d in m['source_sha256'].items():assert h(subprocess.check_output(['git','show',m['source_commit']+':'+n]))==d,n
for role,b in m['binaries'].items():
 for n,d in b['candidate_sources_sha256'].items():
  expected=h(subprocess.check_output(['git','show',m['baseline_commit']+':'+n])) if role=='baseline' else m['source_sha256'][n];assert expected==d
assert len(status)==18 and {(r['repeat'],r['rows'],r['role']) for r in status}=={(i,n,role) for i in range(1,4) for n in [1000,10000,100000] for role in ['baseline','candidate']}
records=[]
for r in status:
 assert r['exit_code']==0 and r['binary_sha256']==m['binaries'][r['role']]['sha256'];d=load(f"{r['role']}-{r['rows']}-{r['repeat']}.json");assert d['rows']==r['rows'] and d['exact_all_rows_readback'] and d['counting_system_allocator'] and not d['latency_qualified'] and not d['rss_qualified'];assert d['output_quota_bytes']==1024*1024*1024
 for name in ['start','seeded','held','released','closed']:
  c=d[name];assert all(type(v)is int and v>=0 for v in c.values());assert c['live']==c['alloc_bytes']-c['free_bytes']+c['realloc_new_bytes']-c['realloc_old_bytes'],(r,name,'allocator ledger does not balance')
 for a,b in zip(['start','seeded','held','released'],['seeded','held','released','closed']):
  assert all(d[b][k]>=d[a][k] for k in d[a] if k!='live'),(r,a,b)
 metrics=dict(seed_live_bytes=d['seeded']['live']-d['start']['live'],query_live_bytes=d['held']['live']-d['seeded']['live'],query_allocations=d['held']['alloc']-d['seeded']['alloc'],query_requested_bytes=d['held']['alloc_bytes']-d['seeded']['alloc_bytes'],query_reallocations=d['held']['realloc']-d['seeded']['realloc'],after_result_drop_live_bytes=d['released']['live']-d['seeded']['live'],after_db_drop_live_bytes=d['closed']['live']-d['start']['live'])
 records.append(dict(**r,row_descriptor_bytes=d['row_descriptor_bytes'],metrics=metrics))
summary={}
for n in [1000,10000,100000]:
 out={}
 for role in ['baseline','candidate']:
  selected=[r for r in records if r['rows']==n and r['role']==role];assert len(selected)==3;out[role]={k:statistics.median(r['metrics'][k] for r in selected) for k in selected[0]['metrics']};out[role]['row_descriptor_bytes']=selected[0]['row_descriptor_bytes'];assert all(r['row_descriptor_bytes']==out[role]['row_descriptor_bytes'] for r in selected)
 out['delta']={k:out['candidate'][k]-out['baseline'][k] for k in out['baseline']};summary[str(n)]=out
q=dict(diagnostic_complete=True,processes=18,allocator_ledger_checks=90,source_candidate_qualified=False,latency_qualified=False,rss_qualified=False,full_objective_achieved=False,summary=summary,records=records);(p/'summary.json').write_text(json.dumps(q,indent=2)+'\n')
for n,r in summary.items():print(n,json.dumps(r['delta']))
print('Independent source/raw allocator ledger audit passed; no timing/RSS/whole-engine acceptance')
