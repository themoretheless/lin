#!/usr/bin/env python3
"""Audit frozen bulk template component source and recompute every paired observation."""
from pathlib import Path
import json,subprocess,hashlib,statistics,math,itertools,argparse,re
ROOT=Path(__file__).resolve().parents[1]
HEAD='84817f2113db7b2ce3f9d46d2894b706b95b662f'
INPUT='benches/results/2026-10-28-cow-bulk-map-template-component'
CASES=['compare/insert_bulk_1k/lin', 'compare/insert_bulk_10k/lin', 'compare/filter_eq/lin', 'compare/update_1row_1k/lin', 'compare/update_1row_100k/lin', 'compare/wal_scalar_full_update/lin', 'compare/wal_scalar_normal_update/lin', 'compare/insert_bulk_10k/sqlite']
def sha(data):return hashlib.sha256(data).hexdigest()
def git(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
def matrix(raw):
 provenance=json.loads((raw/'provenance.json').read_text());records=json.loads((raw/'process-status.json').read_text());expected=set(itertools.product(range(1,13),['baseline','control','candidate'],CASES));assert len(records)==288
 assert {(r['round'],r['mode'],r['case'])for r in records}==expected
 medians={}
 for r in records:
  assert r['exit_code']==0 and math.isfinite(r['elapsed_seconds'])and r['elapsed_seconds']>0
  assert r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
  run=json.loads((raw/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json').read_text());assert run['status']=='complete'and len(run['cases'])==1 and run['cases'][0]['id']==r['case']and len(run['observations'])==8
  values=[]
  for o in run['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
   v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
  medians[r['round'],r['mode'],r['case']]=statistics.median(values)
 summary={};passed=True
 for case in CASES:
  summary[case]={};target=case.startswith('compare/insert_bulk_')and case.endswith('/lin')
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    b,a,c=[medians[i,mode,case]for mode in ['baseline','control','candidate']]
    pairs.append(dict(round=i,baseline_ns=b,control_ns=a,candidate_ns=c,gain_percent=100*(1-c/b),aa_percent=100*(1-a/b)))
   gain=statistics.median(p['gain_percent']for p in pairs);noise=statistics.median(abs(p['aa_percent'])for p in pairs);positive=sum(p['gain_percent']>0 for p in pairs)
   ok=(positive==6 and gain>noise)if target else gain>=-max(2,noise);passed&=ok
   summary[case][label]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
 qualification=dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=288,observations=2304)
 assert json.loads((raw/'qualification.json').read_text())==qualification
 assert json.loads((raw/'summary.json').read_text())==summary
 return dict(audit_complete=True,**qualification,failed_target_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if w['target']),failed_control_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if not w['target']),summary=summary)
def audit(parent):
 raw=parent/'cow-bulk-map-template-component';run=json.loads((parent/'run-metadata.json').read_text());assert run.get('headSha',run.get('head_sha'))==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure']
 frozen=git(INPUT+'/frozen-inputs.json');assert(raw/'frozen-inputs.json').read_bytes()==frozen
 for name,h in json.loads(frozen).items():
  data=git(name);assert sha(data)==h,name
  if name.startswith(INPUT+'/'):assert(raw/name[len(INPUT)+1:]).read_bytes()==data,name
 manifest=json.loads((raw/'input.json').read_text());assert(raw/'baseline-exec.rs').read_bytes()==git('src/exec.rs',manifest['baseline_reference']);assert(raw/'candidate-exec.rs').read_bytes()==git('src/exec.rs',manifest['candidate_reference'])==git('src/exec.rs')
 for name,h in manifest['source_sha256'].items():assert sha(git(name))==h
 tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==manifest['source_sha256']
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'release-tests.log').read_text());assert[sum(int(c[i])for c in counts)for i in range(3)]==[449,0,2]
 prov=json.loads((raw/'provenance.json').read_text());assert prov['source_commit']==HEAD and prov['source_restored']and prov['instrumentation']is False and prov['source_sha256']==manifest['source_sha256']and prov['compare_sha256']==sha(git(INPUT+'/compare.rs'))
 for role in ['baseline','candidate']:assert prov['binaries'][role]['exec_sha256']==sha((raw/f'{role}-exec.rs').read_bytes())
 smokes=json.loads((raw/'smoke-status.json').read_text());assert len(smokes)==16 and {(s['role'],s['case'])for s in smokes}==set(itertools.product(['baseline','candidate'],CASES))and all(s['exit']==0 for s in smokes)
 for s in smokes:
  q=json.loads((raw/'smoke'/(s['role']+'-'+s['case'].replace('/','_'))/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==s['case']and len(q['observations'])==1
 result=matrix(raw);return dict(**result,run_head=HEAD,release_tests=449,ignored_tests=2,correctness_smokes=16)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('result_directory',type=Path);a=p.parse_args();result=audit(a.result_directory.resolve());(a.result_directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
