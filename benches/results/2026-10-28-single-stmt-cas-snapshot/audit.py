from pathlib import Path
import json,hashlib,statistics,math,sys,subprocess
p=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).hexdigest()
frozen=json.loads((p/'measurement-input.json').read_text())
for name,digest in frozen['files_sha256'].items():assert sha((p/name).read_bytes())==digest,name
provenance=json.loads((p/'provenance.json').read_text());tests=json.loads((p/'test-status.json').read_text());inputs=json.loads((p/'input.json').read_text())
assert tests['exit_code']==0 and tests['passed']==449 and tests['source_sha256']==inputs['source_sha256']==provenance['source_sha256']
assert provenance['source_restored'] and provenance['compare_sha256']==frozen['files_sha256']['compare.rs']
for role in ['baseline','candidate']:assert provenance['binaries'][role]['exec_sha256']==frozen['files_sha256'][role+'-exec.rs']
candidate='2633067cb1f59d773de49af403e41f3afc7c5688'
assert provenance['candidate_frozen_commit']==candidate
for name,digest in inputs['source_sha256'].items():
 path=Path(name);assert not path.is_absolute()and'..'not in path.parts
 assert sha(subprocess.check_output(['git','show',candidate+':'+name]))==digest,name
assert sha(subprocess.check_output(['git','show',inputs['baseline_commit']+':src/exec.rs']))==frozen['files_sha256']['baseline-exec.rs']
assert sha(subprocess.check_output(['git','show',candidate+':tests/single_stmt_cas.rs']))==frozen['files_sha256']['single_stmt_cas.rs']
mutant=json.loads((p/'mutant-validation.json').read_text());assert mutant['runtime_rejected']and mutant['source_restored']and mutant['original_exec_sha256']==frozen['files_sha256']['candidate-exec.rs']
assert sha((p/'mutant-exec.rs').read_bytes())==mutant['mutant_exec_sha256']
assert 'multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything ... FAILED'in(p/'mutant-runtime.log').read_text()
records=json.loads((p/'process-status.json').read_text());summary=json.loads((p/'summary.json').read_text());cases=frozen['targets']+frozen['controls']
coordinates=[(r['round'],r['mode'],r['case'])for r in records]
assert len(coordinates)==144 and set(coordinates)=={(n,role,case)for n in range(1,13)for role in ['baseline','control','candidate']for case in cases}
med={};count=0
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
 data=json.loads((p/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json').read_text())
 assert data['status']=='complete'and len(data['cases'])==1 and data['cases'][0]['id']==r['case'] and len(data['observations'])==8
 values=[]
 for o in data['observations']:
  value=float(o['value'])
  assert o['case']==r['case']and o['operations']==1 and o['metric']=='wall' and math.isfinite(value)and value>0
  values.append(value);count+=1
 med[r['round'],r['mode'],r['case']]=statistics.median(values)
assert count==1152
failures=[]
for case in cases:
 assert set(summary[case])=={'series1','series2'}
 for series,start in [('series1',1),('series2',7)]:
  gains=[];noise=[]
  for n in range(start,start+6):
   base=med[n,'baseline',case]
   gains.append(100*(1-med[n,'candidate',case]/base));noise.append(abs(100*(1-med[n,'control',case]/base)))
  gain=statistics.median(gains);aa=statistics.median(noise);positive=sum(x>0 for x in gains);target=case in frozen['targets']
  ok=(positive==6 and gain>aa)if target else gain>=-max(2,aa)
  actual=summary[case][series]
  assert actual['target']==target and actual['passes']==ok and actual['positive_pairs']==positive
  assert math.isclose(actual['gain_percent'],gain,abs_tol=1e-12) and math.isclose(actual['aa_absolute_percent'],aa,abs_tol=1e-12)
  if not ok:failures.append(dict(case=case,series=series,target=target))
q=json.loads((p/'qualification.json').read_text());assert q['measurement_complete'] and q['processes']==144 and q['observations']==1152
assert q['component_gate_passes']==(len(failures)==0) and q['full_engine_qualified'] is False and q['full_objective_achieved'] is False
result=dict(audit_complete=True,processes=144,observations=1152,failed_target_windows=sum(x['target'] for x in failures),failed_control_windows=sum(not x['target'] for x in failures),failures=failures,component_gate_passes=len(failures)==0)
print(json.dumps(result,indent=2))
