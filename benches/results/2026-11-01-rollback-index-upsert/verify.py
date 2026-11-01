from pathlib import Path
import hashlib,json,math,re,subprocess
p=Path(__file__).resolve().parent;load=lambda name:json.loads((p/name).read_text());sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
def median(values):
 s=sorted(values);return(s[(len(s)-1)//2]+s[len(s)//2])/2
m=load('input.json');meta=load('provenance.json');tests=load('test-status.json');negative=load('mutant-status.json');baseline_test=load('baseline-regression-status.json')
assert meta['source_restored']and meta['instrumentation']is False
assert sha('benches/compare.rs')==meta['original_compare_sha256']and sha(p/'compare.rs')==meta['compiled_compare_sha256']
assert meta['source_sha256']==m['source_sha256']and all(sha(f)==h for f,h in m['source_sha256'].items())
assert sha('tests/partial_index_mutation.rs')==m['test_sha256']
assert tests['exit_code']==0 and tests['release_passed']==459 and tests['release_ignored']==2 and tests['source_sha256']==m['source_sha256']
assert negative['source_restored']and negative['compiled_runtime_rejected']and negative['exit_code']==101
assert baseline_test['source_restored']and baseline_test['baseline_regression_passed']and baseline_test['exit_code']==0
old=(p/'baseline-store.rs').read_text();new=(p/'candidate-store.rs').read_bytes();a=old.index('    pub(crate) fn restore_row_owned(');b=old.index('\n    pub fn restore_rows(',a);part=old[a:b];needle='        self.index_remove_row(collection, idx, &current);\n';assert part.count(needle)==1
assert new==(old[:a]+part.replace(needle,'        // insert_at replaces the existing slot key, including partial-index removal.\n')+old[b:]).encode()
assert (p/'baseline-store.rs').read_bytes()==subprocess.check_output(['git','show',m['baseline_commit']+':src/store.rs'])
assert sha(p/'candidate-store.rs')==m['source_sha256']['src/store.rs']
for role in ['baseline','candidate']:
 assert sha(meta['binaries'][role]['path'])==meta['binaries'][role]['sha256']
 assert sha(p/(role+'-store.rs'))==meta['binaries'][role]['store_sha256']
cases=load('cases.json');assert len(cases)==len(set(cases))==14;targets=set(cases[:5]);assert targets=={f'compare/tx_rollback_1row_{n}/lin'for n in ['1k','10k','100k']}|{f'compare/durable_rollback_1row_{n}/lin'for n in ['1k','10k']}
smokes=load('qualification-smoke-status.json');assert len(smokes)==28 and {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['baseline','candidate']for c in cases}
for r in smokes:
 assert r['exit']==0;d=load('qualification-smoke/'+r['role']+'-'+r['case'].replace('/','_')+'/run.json');assert d['status']=='complete'and len(d['observations'])==1 and d['cases'][0]['id']==r['case']
records=load('process-status.json');assert len(records)==504
order=[]
for i in range(1,13):
 for index,c in enumerate(cases[i%14:]+cases[:i%14]):
  roles=['baseline','control','candidate'];shift=(i+index)%3;order.extend((i,r,c)for r in roles[shift:]+roles[:shift])
assert [(r['round'],r['mode'],r['case'])for r in records]==order
values={};ids=set();total=0
for r in records:
 binary=meta['binaries']['baseline'if r['mode']=='control'else r['mode']];assert r['exit_code']==0 and r['binary_sha256']==binary['sha256']
 dest=p/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')
 assert r['command']==[binary['path'],'--filter',r['case'],'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
 d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
 assert d['id']not in ids;ids.add(d['id']);assert d['provenance']['samples']=='8'and d['provenance']['warmup_ns']=='0'
 samples=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
  value=float(o['value']);assert math.isfinite(value)and value>0;samples.append(value);total+=1
 values[r['round'],r['mode'],r['case']]=median(samples)
assert total==4032
summary=load('summary.json');assert set(summary)==set(cases);accepted=True
for c in cases:
 assert set(summary[c])=={'series1','series2'}
 for label,start in [('series1',1),('series2',7)]:
  pairs=[]
  for i in range(start,start+6):
   a,b,d=[values[i,r,c]for r in ['baseline','control','candidate']];pairs.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=d,gain_percent=100*(1-d/a),aa_percent=100*(1-b/a)))
  gain=median(x['gain_percent']for x in pairs);noise=median(abs(x['aa_percent'])for x in pairs);positive=sum(x['gain_percent']>0 for x in pairs);passed=(positive==6 and gain>noise)if c in targets else gain>=-max(2,noise);accepted &=passed
  assert summary[c][label]==dict(target=c in targets,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,pairs=pairs)
q=load('qualification.json');assert q==dict(measurement_complete=True,component_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,processes=504,observations=4032)
(p/'independent-audit.json').write_text(json.dumps(dict(audit_complete=True,processes=504,observations=4032,component_gate_passes=accepted,full_objective_achieved=False),indent=2)+'\n')
print('Independent audit PASS: 504 processes/4032 observations; component gate:',accepted)
