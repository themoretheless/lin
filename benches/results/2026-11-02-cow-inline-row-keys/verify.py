from pathlib import Path
import hashlib,json,math,statistics,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
sha=lambda b:hashlib.sha256(b).hexdigest()
m=json.loads((p/'provenance.json').read_text());status=json.loads((p/'status.json').read_text());cases=json.loads((p/'cases.json').read_text())
records=json.loads((p/'process-status.json').read_text());smokes=json.loads((p/'smoke-status.json').read_text());reported=json.loads((p/'summary.json').read_text())
assert len(cases)==12 and len(set(cases))==12 and len(records)==216 and len(smokes)==24
assert m['source_restored'] and m['exit_code']==0 and m['performance_qualified']==False
assert sha((p/'compare.rs').read_bytes())==m['harness_sha256']
assert m['harness_change']=='durable_json_row owned String boundary only; timed workload unchanged'

original=(root/'benches/results/2026-11-02-rollback-fts-field-impact-bench/compare.rs').read_bytes()
assert sha(original)==m['original_harness_sha256']
assert original.count(b'(k.clone(),value)')==1
assert original.replace(b'(k.clone(),value)',b'(k.to_string(),value)')==(p/'compare.rs').read_bytes()
prior=json.loads((root/'benches/results/2026-11-02-cow-package-codegen1-local/provenance.json').read_text())
assert m['rustc']==prior['rustc'] and m['baseline']==prior['baseline']
for name,h in m['build_sha256'].items():assert sha((root/name).read_bytes())==h

assert status==dict(processes=216,observations=1728,smokes=24,measurement_complete=True,source_restored=True,performance_qualification=False,adopted=False)
plan=schedule([dict(id=c)for c in cases],['baseline','control','candidate'],rounds=6)
assert [(r['round'],r['role'],r['case'])for r in records]==[(r['round'],r['role'],r['workload'])for r in plan]
orders_from_records(records,cases,['baseline','control','candidate'],'case',rounds=6)
assert {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['baseline','candidate']for c in cases}
med={};observations=0
for smoke,rows in [(True,smokes),(False,records)]:
 for r in rows:
  b=m['candidate' if r['role']=='candidate' and not r['case'].endswith('/duckdb') else 'baseline']
  assert r['exit_code']==0 and r['binary_sha256']==b['sha256'] and sha(Path(b['path']).read_bytes())==b['sha256']
  n=1 if smoke else 8
  dest=p/'smokes'/f"{r['role']}-{cases.index(r['case']):02}"if smoke else p/'raw'/f"{r['round']:02}-{r['role']}"/r['case'].replace('/','_')
  assert r['command']==[b['path'],'--filter',r['case'],'--samples',str(n),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  raw=json.loads((dest/'run.json').read_text());assert raw['status']=='complete'and len(raw['cases'])==1 and raw['cases'][0]['id']==r['case']and len(raw['observations'])==n
  values=[]
  for o in raw['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
   v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
  value=statistics.median(values);assert value==r['median_ns']
  if not smoke:med[r['round'],r['role'],r['case']]=value;observations+=len(values)
assert observations==1728
rebuilt={}
for case in cases:
 pairs=[]
 for rnd in range(1,7):
  a,b,c=(med[rnd,role,case]for role in ['baseline','control','candidate'])
  pairs.append(dict(round=rnd,baseline=a,control=b,candidate=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
 rebuilt[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
assert rebuilt==reported
original=json.loads((p/'environment.json').read_text())['source_sha256']
assert original=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
result=dict(audit_complete=True,processes=216,observations=1728,smokes=24,source_unchanged=True,binary_hashes_checked=True,exact_commands_checked=True,balanced_order_checked=True,raw_numerical_recompute_identical=True,performance_qualification=False)
(p/'independent-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
