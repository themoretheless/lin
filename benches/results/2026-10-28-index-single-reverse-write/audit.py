from pathlib import Path
import json,hashlib,statistics as st,math,sys,subprocess
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
h=lambda b:hashlib.sha256(b).hexdigest()
frozen=json.loads((p/'measurement-input.json').read_text())
for name,digest in frozen['files_sha256'].items():assert h((p/name).read_bytes())==digest,name
inputs=json.loads((p/'input.json').read_text());provenance=json.loads((p/'provenance.json').read_text());tests=json.loads((p/'test-status.json').read_text())
assert tests['exit_code']==0 and tests['passed']==445 and tests['source_sha256']==inputs['source_sha256']==provenance['source_sha256']
assert provenance['source_restored'] and provenance['gate_sha256']==frozen['files_sha256']['gate.rs']
assert h(subprocess.check_output(['git','show',inputs['baseline_commit']+':src/index.rs']))==frozen['files_sha256']['baseline-index.rs']
for role in ['baseline','candidate']:assert provenance['binaries'][role]['index_sha256']==frozen['files_sha256'][role+'-index.rs']
records=json.loads((p/'process-status.json').read_text());assert len(records)==36 and {(r['round'],r['mode'])for r in records}=={(i,m)for i in range(1,13)for m in ['baseline','control','candidate']}
med={};count=0
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
 rows=[json.loads(line)for line in(p/f"{r['round']:02}-{r['mode']}.jsonl").read_text().splitlines()]
 assert len(rows)==64 and {(x['width'],x['changed'],x['sample'])for x in rows}=={(w,c,s)for w in [1,2,3,8]for c in [False,True]for s in range(8)}
 for x in rows:assert x['operations']==20000 and x['complete_forward_reverse_oracle'] is True and math.isfinite(x['ns_per_update'])and x['ns_per_update']>0
 for w in [1,2,3,8]:
  for c in [False,True]:med[r['round'],r['mode'],w,c]=st.median(x['ns_per_update']for x in rows if x['width']==w and x['changed']==c)
 count+=len(rows)
summary=json.loads((p/'summary.json').read_text());assert len(summary)==8;failures=[]
for w in [1,2,3,8]:
 for c in [False,True]:
  case=f'width{w}/'+('changed'if c else'unchanged');assert set(summary[case])=={'series1','series2'}
  for series,start in [('series1',1),('series2',7)]:
   gains=[100*(1-med[i,'candidate',w,c]/med[i,'baseline',w,c])for i in range(start,start+6)]
   noise=[abs(100*(1-med[i,'control',w,c]/med[i,'baseline',w,c]))for i in range(start,start+6)]
   gain=st.median(gains);aa=st.median(noise);positive=sum(x>0 for x in gains);passed=(positive==6 and gain>aa)if c else gain>=-max(2,aa)
   actual=summary[case][series];assert actual['target']==c and actual['passes']==passed and actual['positive_pairs']==positive
   assert math.isclose(actual['gain_percent'],gain,abs_tol=1e-12)and math.isclose(actual['aa_absolute_percent'],aa,abs_tol=1e-12)
   if not passed:failures.append(dict(case=case,series=series,target=c))
q=json.loads((p/'qualification.json').read_text());assert count==q['observations']==2304 and q['processes']==36 and q['measurement_complete']
assert q['component_gate_passes']==(not failures)and q['full_engine_qualified'] is False and q['full_objective_achieved'] is False
print(json.dumps(dict(audit_complete=True,processes=36,observations=count,failed_target_windows=sum(x['target']for x in failures),failed_control_windows=sum(not x['target']for x in failures),failures=failures),indent=2))
