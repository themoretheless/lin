#!/usr/bin/env python3
"""Independently audit frozen UPDATE patch clone-elision source and paired observations."""
from pathlib import Path
import json,subprocess,hashlib,statistics,math,itertools,argparse,re
ROOT=Path(__file__).resolve().parents[1]
HEAD='0ffe738f4959eee769cf6e261066be426a961dee'
INPUT='benches/results/2026-11-01-update-patch-clone-elision'
CASES=['compare/update_1row_1k/lin', 'compare/update_1row_10k/lin', 'compare/update_1row_100k/lin', 'compare/durable_update_1row_1k/lin', 'compare/durable_update_1row_10k/lin', 'compare/durable_delete_1row_10k/lin', 'compare/durable_commit_1row_10k/lin', 'compare/durable_rollback_1row_10k/lin', 'compare/wal_scalar_full_update/lin', 'compare/wal_scalar_normal_update/lin', 'compare/insert_bulk_10k/lin', 'compare/filter_eq/lin', 'compare/filter_eq/duckdb', 'compare/update_1row_1k/sqlite']
def sha(data):return hashlib.sha256(data).hexdigest()
def git(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
def matrix(raw):
 provenance=json.loads((raw/'provenance.json').read_text());records=json.loads((raw/'process-status.json').read_text());expected=set(itertools.product(range(1,13),['baseline','control','candidate'],CASES));assert len(records)==504
 assert {(r['round'],r['mode'],r['case'])for r in records}==expected
 medians={}
 for r in records:
  assert r['exit_code']==0 and math.isfinite(r['elapsed_seconds'])and r['elapsed_seconds']>0
  assert r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
  run=json.loads((raw/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json').read_text());assert run['status']=='complete'and len(run['cases'])==1 and run['cases'][0]['id']==r['case']and len(run['observations'])==8
  values=[]
  for o in run['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
   v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
  medians[r['round'],r['mode'],r['case']]=statistics.median(values)
 summary={};passed=True
 for case in CASES:
  summary[case]={};target=case in ['compare/update_1row_1k/lin', 'compare/update_1row_10k/lin', 'compare/update_1row_100k/lin']
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    b,a,c=[medians[i,mode,case]for mode in ['baseline','control','candidate']]
    pairs.append(dict(round=i,baseline_ns=b,control_ns=a,candidate_ns=c,gain_percent=100*(1-c/b),aa_percent=100*(1-a/b)))
   gain=statistics.median(p['gain_percent']for p in pairs);noise=statistics.median(abs(p['aa_percent'])for p in pairs);positive=sum(p['gain_percent']>0 for p in pairs)
   ok=(positive==6 and gain>noise)if target else gain>=-max(2,noise);passed&=ok
   summary[case][label]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
 qualification=dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=504,observations=4032)
 assert json.loads((raw/'qualification.json').read_text())==qualification
 assert json.loads((raw/'summary.json').read_text())==summary
 return dict(audit_complete=True,**qualification,failed_target_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if w['target']),failed_control_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if not w['target']),summary=summary)
def source_map(ref,prefix="src"):
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--',prefix],cwd=ROOT,text=True).splitlines()
 return {n:sha(git(n,ref))for n in names if n.endswith('.rs')}
def audit(parent):
 raw=parent/'update-patch-clone-elision-component';run=json.loads((parent/'run-metadata.json').read_text())
 assert run['headSha']==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure']
 frozen=json.loads(git(INPUT+'/frozen-inputs.json'));assert(raw/'frozen-inputs.json').read_bytes()==git(INPUT+'/frozen-inputs.json')
 for name,h in frozen.items():
  assert sha(git(name))==h,name
  if name.startswith(INPUT+'/'):assert sha((raw/name[len(INPUT)+1:]).read_bytes())==h,name
 retry=json.loads(git(INPUT+'/infrastructure-retry.json'));old=retry['original_frozen_head']
 assert old=='f38e3178c003a234acbad6b8baff44d316407916'and retry['original_run_id']==41249170021
 assert retry['timing_processes_before_retry']==retry['timing_observations_before_retry']==0 and retry['runtime_sources_unchanged']and retry['thresholds_unchanged']
 assert source_map(old)==source_map(HEAD)and source_map(old,'tests')==source_map(HEAD,'tests')
 for name in ['Cargo.toml','Cargo.lock',*[INPUT+'/'+n for n in ['compare.rs','run.py','analyze.py','build.py','smoke.py','test.py','test-protocol.py','check-mutants.py','input.json','performance-plan.json']]]:
  assert git(name,old)==git(name),name
 previous='benches/results/2026-11-01-update-patch-clone-elision-linux-attempt1/'
 first=json.loads(git(previous+'functional-audit.json'));assert first['audit_complete']and first['infrastructure_failure']and first['timing_processes']==first['timing_observations']==0
 assert sha(git(previous+'raw.tar.gz'))==retry['original_attempt_archive_sha256']
 assert 'fetch-depth: 0'in git('.github/workflows/update-patch-clone-elision-component.yml').decode()
 manifest=json.loads(git(INPUT+'/input.json'));baseline=source_map(manifest['baseline_reference']);candidate=source_map(manifest['candidate_reference'])
 assert manifest['candidate_reference']==subprocess.check_output(['git','rev-parse','ed3098197'],cwd=ROOT,text=True).strip()
 assert candidate==source_map(HEAD)==manifest['source_sha256']
 assert set(candidate)==set(baseline)
 assert {n for n in baseline if baseline[n]!=candidate[n]}=={'src/exec.rs'}
 assert manifest['tests_sha256']==source_map(HEAD,'tests')
 for n in ['Cargo.toml','Cargo.lock']:assert git(n)==git(n,manifest['baseline_reference'])
 tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==candidate and tests['release_passed']==455 and tests['release_ignored']==2
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'release-tests.log').read_text());assert[sum(int(c[i])for c in counts)for i in range(3)]==[455,0,2]
 assert 'test update_patch_preserves_explicit_and_derived_hash_guards ... ok'in(raw/'release-tests.log').read_text()
 prov=json.loads((raw/'provenance.json').read_text());assert prov['source_commit']==HEAD and prov['source_restored']and prov['instrumentation']is False and prov['source_sha256']==candidate and prov['compare_sha256']==sha(git(INPUT+'/compare.rs'))
 for role,expected in [('baseline',baseline),('candidate',candidate)]:
  assert prov['binaries'][role]['role_source_sha256']==expected
  assert 'Finished `bench` profile'in(raw/f'build-{role}.log').read_text()
 assert prov['binaries']['baseline']['sha256']!=prov['binaries']['candidate']['sha256']
 smokes=json.loads((raw/'qualification-smoke-status.json').read_text());assert len(smokes)==28 and {(s['role'],s['case'])for s in smokes}==set(itertools.product(['baseline','candidate'],CASES))and all(s['exit']==0 for s in smokes)
 for s in smokes:
  q=json.loads((raw/'qualification-smoke'/(s['role']+'-'+s['case'].replace('/','_'))/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==len(q['observations'])==1 and q['cases'][0]['id']==s['case']and q['environment']['os']=='linux'
  assert q['observations'][0]['availability']['state']=='available'and q['observations'][0]['case']==s['case']
 mutants=json.loads((raw/'mutant-status.json').read_text());assert mutants['source_restored']and len(mutants['checks'])==2
 assert {c['label']for c in mutants['checks']}=={'guard-derived-hash','skip-explicit-hash-guard'}
 original=git('src/exec.rs').decode();old='if k == "hash" && !explicit_hash {';assert original.count(old)==1
 for label,new in [('guard-derived-hash','if false {'),('skip-explicit-hash-guard','if k == "hash" {')]:
  expected=original.replace(old,new).encode();assert(raw/(label+'-mutant.rs')).read_bytes()==expected
  check=next(c for c in mutants['checks']if c['label']==label);assert check['exit_code']==101 and check['compiled_runtime_rejected']and check['mutant_sha256']==sha(expected)
  log=(raw/(label+'-runtime.log')).read_text();assert 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and 'test update_patch_preserves_explicit_and_derived_hash_guards ... FAILED'in log
 environment=json.loads((raw/'environment.json').read_text());assert 'Linux'in environment['platform']and 'host: x86_64-unknown-linux-gnu'in environment['rustc']and 'rustc 1.96.0 'in environment['rustc']and environment['same_runtime_all_roles']and not environment['memory_measured']
 assert 'FSTYPE'in(raw/'linux-environment.txt').read_text()
 return dict(**matrix(raw),run_head=HEAD,release_tests=455,ignored_tests=2,correctness_smokes=28,runtime_mutants_rejected=2,role_source_manifests_exact=True,memory_measured=False)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('result_directory',type=Path);args=p.parse_args();result=audit(args.result_directory.resolve());(args.result_directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items()if k!='summary'},indent=2))
