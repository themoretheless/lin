#!/usr/bin/env python3
"""Audit frozen positive-zero WAL source, legacy goldens and every paired observation."""
from pathlib import Path
import json,subprocess,hashlib,statistics,math,itertools,argparse,re
ROOT=Path(__file__).resolve().parents[1]
HEAD='9fb4c1ed321f73387d2f074e0547e9f5731d9e15'
INPUT='benches/results/2026-11-01-wal-positive-zero-component'
CASES=['compare/durable_update_1row_1k/lin', 'compare/durable_update_1row_10k/lin', 'compare/durable_delete_1row_1k/lin', 'compare/durable_delete_1row_10k/lin', 'compare/durable_commit_1row_10k/lin', 'compare/durable_rollback_1row_10k/lin', 'compare/wal_scalar_full_update/lin', 'compare/wal_scalar_normal_update/lin', 'compare/update_1row_1k/lin', 'compare/insert_bulk_10k/lin', 'compare/filter_eq/lin', 'compare/durable_update_1row_10k/sqlite_wal']
def sha(data):return hashlib.sha256(data).hexdigest()
def git(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
def matrix(raw):
 provenance=json.loads((raw/'provenance.json').read_text());records=json.loads((raw/'process-status.json').read_text());expected=set(itertools.product(range(1,13),['baseline','control','candidate'],CASES));assert len(records)==432
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
  summary[case]={};target=case in ['compare/durable_update_1row_1k/lin', 'compare/durable_update_1row_10k/lin', 'compare/durable_delete_1row_1k/lin', 'compare/durable_delete_1row_10k/lin', 'compare/durable_commit_1row_10k/lin']
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    b,a,c=[medians[i,mode,case]for mode in ['baseline','control','candidate']]
    pairs.append(dict(round=i,baseline_ns=b,control_ns=a,candidate_ns=c,gain_percent=100*(1-c/b),aa_percent=100*(1-a/b)))
   gain=statistics.median(p['gain_percent']for p in pairs);noise=statistics.median(abs(p['aa_percent'])for p in pairs);positive=sum(p['gain_percent']>0 for p in pairs)
   ok=(positive==6 and gain>noise)if target else gain>=-max(2,noise);passed&=ok
   summary[case][label]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
 qualification=dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=432,observations=3456)
 assert json.loads((raw/'qualification.json').read_text())==qualification
 assert json.loads((raw/'summary.json').read_text())==summary
 return dict(audit_complete=True,**qualification,failed_target_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if w['target']),failed_control_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if not w['target']),summary=summary)
def source_map(ref):
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src'],cwd=ROOT,text=True).splitlines()
 return {n:sha(git(n,ref))for n in names if n.endswith('.rs')}
def audit(parent):
 raw=parent/'wal-positive-zero-component';run=json.loads((parent/'run-metadata.json').read_text())
 assert run['headSha']==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure']
 frozen=git(INPUT+'/frozen-inputs.json');assert(raw/'frozen-inputs.json').read_bytes()==frozen
 for name,h in json.loads(frozen).items():
  data=git(name);assert sha(data)==h,name
  if name.startswith(INPUT+'/'):assert(raw/name[len(INPUT)+1:]).read_bytes()==data,name
 manifest=json.loads((raw/'input.json').read_text());assert manifest['cases']==CASES and manifest['targets']==CASES[:5] and manifest['processes']==432 and manifest['observations']==3456
 assert manifest['source_sha256']==source_map(manifest['candidate_reference'])==source_map(HEAD)
 baseline=source_map(manifest['baseline_reference'])
 assert set(manifest['source_sha256'])-set(baseline)=={'src/persist/positive_zero.rs'}
 for name,h in baseline.items():
  if name!='src/persist.rs':assert manifest['source_sha256'][name]==h,'unrelated baseline change'
 for role,key in [('baseline','baseline_reference'),('candidate','candidate_reference')]:assert(raw/f'{role}-persist.rs').read_bytes()==git('src/persist.rs',manifest[key])
 assert(raw/'candidate-positive_zero.rs').read_bytes()==git('src/persist/positive_zero.rs')
 tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==manifest['source_sha256']and tests['release_passed']==457 and tests['release_ignored']==2
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'release-tests.log').read_text());assert[sum(int(c[i])for c in counts)for i in range(3)]==[457,0,2]
 prov=json.loads((raw/'provenance.json').read_text());assert prov['source_commit']==HEAD and prov['source_restored']and prov['instrumentation']is False and prov['source_sha256']==manifest['source_sha256']and prov['compare_sha256']==sha(git(INPUT+'/compare.rs'))
 for role,expected in [('baseline',baseline),('candidate',manifest['source_sha256'])]:
  assert prov['binaries'][role]['role_source_sha256']==expected
  assert 'Finished `bench` profile'in(raw/f'build-{role}.log').read_text()
 assert prov['binaries']['baseline']['sha256']!=prov['binaries']['candidate']['sha256']
 smokes=json.loads((raw/'qualification-smoke-status.json').read_text());assert len(smokes)==24 and {(s['role'],s['case'])for s in smokes}==set(itertools.product(['baseline','candidate'],CASES))and all(s['exit']==0 for s in smokes)
 for s in smokes:
  q=json.loads((raw/'qualification-smoke'/(s['role']+'-'+s['case'].replace('/','_'))/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==s['case']and len(q['observations'])==1
  assert q['observations'][0]['availability']['state']=='available'and q['observations'][0]['case']==s['case']and q['environment']['os']=='linux'
 mutants=json.loads((raw/'mutant-status.json').read_text());assert mutants['all_sources_restored']and mutants['source_sha256']==manifest['source_sha256']and len(mutants['checks'])==2
 for label,name,old,new,test in [('negative-zero','src/persist/positive_zero.rs','if self.0.to_bits() == 0 {','if self.0 == 0.0 {','positive_zero_record_shapes_preserve_canonical_bits_and_size'),('canonical-limit','src/persist.rs','buf.len().checked_add(extra)','buf.len().checked_add(0)','positive_zero_canonical_payload_and_envelope_limits_reject_before_write')]:
  original=git(name).decode();assert original.count(old)==1;expected=original.replace(old,new).encode();assert(raw/(label+'-mutant.rs')).read_bytes()==expected
  item=next(c for c in mutants['checks']if c['label']==label);assert item['source_file']==name and item['mutant_sha256']==sha(expected)and item['runtime_exit']==101 and item['compiled_runtime_rejected']and item['source_restored']
  text=(raw/(label+'-runtime.log')).read_text();assert 'running 1 test'in text and 'test result: FAILED. 0 passed; 1 failed;'in text and 'Finished `release` profile'in text
  assert re.search(r'test [^\n]*'+re.escape(test)+r' \.\.\. FAILED',text)
 fixture=json.loads((raw/'fixture-manifest.json').read_text());assert fixture['legacy_reference']=='04749994b9ca7a090476b95401f0f7166d8b870a'and fixture['legacy_cases_verified']==8
 assert(raw/'fixture-manifest.json').read_bytes()==git('benches/results/2026-11-01-wal-positive-zero/fixture-manifest.json',manifest['candidate_reference'])
 legacy=json.loads(git('benches/results/2026-11-01-wal-positive-zero/legacy-status.json',manifest['candidate_reference']));assert legacy['legacy_cases_verified']==8 and legacy['legacy_runtime_exit']==0 and legacy['current_sources_unchanged']and legacy['legacy_sources_unchanged']
 assert legacy['source_sha256']==manifest['source_sha256']
 assert legacy['legacy_build_exit']==legacy['current_build_exit']==legacy['current_runtime_exit']==0
 for name,h in legacy['legacy_source_sha256'].items():assert sha(git(name,fixture['legacy_reference']))==h,'legacy reader source mismatch'
 for name in ['Cargo.toml','Cargo.lock']:assert git(name,manifest['baseline_reference'])==git(name),'dependency or feature change'
 assert sha((raw/'legacy-qualified-fixtures.tar.gz').read_bytes())==fixture['archive_sha256']
 import tarfile
 with tarfile.open(raw/'legacy-qualified-fixtures.tar.gz')as tf:
  members=tf.getmembers();assert len(members)==32 and {m.name for m in members}==set(fixture['files_sha256'])
  for member in members:
   assert member.isfile()and Path(member.name).name==member.name
   h=fixture['files_sha256'][member.name];assert sha(tf.extractfile(member).read())==h
   assert sha((raw/'qualified-golden-input'/member.name).read_bytes())==sha((raw/'generated-fixtures'/member.name).read_bytes())==h
 golden=json.loads((raw/'golden-status.json').read_text());assert golden['legacy_reference']==fixture['legacy_reference']and golden['golden_members_verified']==32 and golden['production_qualified_cases_verified']==8 and golden['source_restored']and golden['build_exit']==golden['runtime_exit']==0
 outcomes=[json.loads(s)for s in(raw/'golden-outcomes.jsonl').read_text().splitlines()];assert len(outcomes)==8 and {x['case']for x in outcomes}=={'update','delete','batch','mixed_batch','deep_batch','empty_vector','negative_zero','ieee_bits'}and all(x['current_replay_verified']and not x['performance_qualified']for x in outcomes)
 environment=json.loads((raw/'environment.json').read_text());assert 'Linux'in environment['platform']and 'host: x86_64-unknown-linux-gnu'in environment['rustc']and 'rustc 1.96.0 'in environment['rustc']and environment['same_runtime_all_roles']and not environment['memory_measured']
 assert 'FSTYPE'in(raw/'linux-environment.txt').read_text()
 return dict(**matrix(raw),run_head=HEAD,release_tests=457,ignored_tests=2,correctness_smokes=24,legacy_golden_cases=8,legacy_golden_files=32,runtime_mutants_rejected=2,role_source_manifests_exact=True,memory_measured=False)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('result_directory',type=Path);a=p.parse_args();result=audit(a.result_directory.resolve());(a.result_directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items()if k!='summary'},indent=2))
