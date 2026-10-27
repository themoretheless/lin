from pathlib import Path
import json,hashlib,math,re
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text())
def digest(f):return hashlib.sha256(Path(f).read_bytes()).hexdigest()
def middle(xs):
 v=sorted(xs);return(v[(len(v)-1)//2]+v[len(v)//2])/2
cases=load('cases.json');records=load('process-status.json');summary=load('summary.json');peers=load('peer-comparisons.json');q=load('qualification.json');meta=load('provenance.json');env=load('environment.json')
assert len(cases)==len(set(cases))==67
assert len(records)==2412 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,m,c)for i in range(1,13)for m in ['baseline','control','candidate']for c in cases}
assert meta['source_restored']and meta['instrumentation']is False and meta['source_sha256']
assert all(digest(f)==h for f,h in meta['source_sha256'].items())
assert digest('benches/compare.rs')==meta['original_compare_sha256']
assert digest(p/'compare.rs')==meta['compiled_compare_sha256']
for source,digest_expected in meta['candidate_sources_sha256'].items():assert digest(p/('candidate-'+Path(source).name))==digest_expected
assert env['runtime_unchanged_all_roles']
status=load('status.json');assert len(status)==36 and {s['run']for s in status}=={f'{i:02}-{m}'for i in range(1,13)for m in ['baseline','control','candidate']}and all(s['exit_code']==0 and s['subprocesses']==67 for s in status)
tests=load('test-status.json');assert tests['exit_code']==0 and tests['source_restored']and not tests['performance_qualified']
mutant=load('mutant-status.json');assert mutant['expected_failure']and mutant['runtime_assertion_caught']and mutant['exit_code']!=0
med={};rss={};total=0
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==meta['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']and r['glibc_tunables']==env['allocator_environment']['GLIBC_TUNABLES']
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}';d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
 vs=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['operations']==1 and o['metric']=='wall'
  value=float(o['value']);assert math.isfinite(value)and value>0;vs.append(value);total+=1
 k=r['round'],r['mode'],r['case'];med[k]=middle(vs)
 value=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert value==r['peak_rss_kib']>0;rss[k]=value
assert total==19296 and set(summary)==set(cases)
targets={f'compare/durable_rollback_1row_{size}/lin'for size in ['1k','10k']}|{f'compare/tx_rollback_1row_{size}/lin'for size in ['1k','10k','100k']};assert targets<=set(cases)
accepted=True;peer_expected=[]
for c in cases:
 assert set(summary[c])=={'series1','series2'}
 for label,start in [('series1',1),('series2',7)]:
  rounds=range(start,start+6);rows=[]
  for i in rounds:
   a=med[i,'baseline',c];b=med[i,'control',c];d=med[i,'candidate',c];rows.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=d,gain_percent=100*(1-d/a),aa_percent=100*(1-b/a)))
  gain=middle(r['gain_percent']for r in rows);aa=middle(abs(r['aa_percent'])for r in rows);positive=sum(r['gain_percent']>0 for r in rows)
  passed=(positive==6 and gain>aa)if c in targets else gain>=-max(2,aa);accepted &=passed
  wanted=dict(target=c in targets,gain_percent=gain,aa_absolute_percent=aa,positive_pairs=positive,passes=passed,process_pairs=rows,peak_rss_kib={m:middle(rss[i,m,c]for i in rounds)for m in ['baseline','control','candidate']})
  assert summary[c][label]==wanted,(c,label)
  if c.endswith('/lin'):
   for peer in ['sqlite','sqlite_delete','sqlite_wal','duckdb']:
    peer_case=c[:-3]+peer
    if peer_case not in cases:continue
    ratios=[med[i,'candidate',c]/med[i,'candidate',peer_case]for i in rounds]
    peer_expected.append(dict(case=c,peer=peer,series=label,pairs=6,median_ratio=middle(ratios),lin_wins=sum(r<1 for r in ratios),process_ratios=ratios))
assert peers==peer_expected and len(peers)==76
assert q['warm_diagnostic_gate_passes']is True
assert q['measurement_complete']and q['source_candidate_qualified']==accepted and q['full_objective_achieved']is False
assert q['target_cases']==5 and q['control_cases']==62 and q['processes']==2412 and q['observations']==total
assert q['comparisons']==sum(r['pairs']for r in peers)and q['candidate_peer_wins']==sum(r['lin_wins']for r in peers)
print('Independent raw audit PASS: 2412 processes /19296 observations; source gate:',accepted)
