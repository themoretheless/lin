#!/usr/bin/env python3
"""Independently audit frozen bulk embedding phase scopes and observer calibration."""
from pathlib import Path
import argparse,subprocess,json,hashlib,statistics as st,itertools,math,re
ROOT=Path(__file__).resolve().parents[1];HEAD='fde0cf63b054b79692641682c7755870cf672b4a';INPUT='benches/results/2026-10-28-bulk-embedding-phases'
CASES=['compare/insert_bulk_1k/lin','compare/insert_bulk_10k/lin'];ROLES=['original','control','disabled','traced'];NAMES=['unused_parent','text_collect','refs','lowercase','feature_hash','normalize','vector_copy','scratch_reset','row_install']
def sha(data):return hashlib.sha256(data).hexdigest()
def git(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
def matrix(raw):
 records=json.loads((raw/'process-status.json').read_text());provenance=json.loads((raw/'provenance.json').read_text());assert len(records)==48 and {(r['round'],r['role'],r['case'])for r in records}==set(itertools.product(range(1,7),ROLES,CASES));med={};traces={};count=0
 for r in records:
  assert r['exit']==0 and math.isfinite(r['elapsed_seconds'])and r['elapsed_seconds']>0 and r['binary_sha256']==provenance['binaries']['original'if r['role']in ['original','control']else'instrumented']['sha256'];dest=raw/f"{r['round']:02}-{r['role']}"/r['case'].replace('/','_');q=json.loads((dest/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==r['case']and len(q['observations'])==8
  assert q['cases'][0]['contract']['lifecycle']=='fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64';values=[]
  for o in q['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1;v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);count+=1
  med[r['round'],r['role'],r['case']]=st.median(values);trace=dest.with_suffix('.trace.json')
  if r['role']=='traced':
   data=trace.read_bytes();assert sha(data)==r['trace_sha256'];t=json.loads(data);assert t['phase_names']==NAMES and t['timing_qualified']is False and 8<=len(t['records'])<=128
   for e in t['records']:
    assert type(e['wall_ns'])is int and e['wall_ns']>0 and e['phase_calls']==[0,1,1]+[1000 if r['case']==CASES[0]else 10000]*5+[1] and all(type(v)is int for v in e['phase_calls'])and len(e['phase_ns'])==9 and e['phase_ns'][0]==0 and all(type(v)is int and v>=0 for v in e['phase_ns'])and sum(e['phase_ns'])<=e['wall_ns']
   traces[r['round'],r['case']]=t['records']
  else:assert r['trace_sha256']is None and not trace.exists()
 assert count==384;summary={}
 for case in CASES:
  aa=[];disabled=[];traced=[];fractions=[];residual=[];invocations=0
  for round in range(1,7):
   o,c,d,t=[med[round,role,case]for role in ROLES];aa.append(abs(100*(1-c/o)));disabled.append(d/o);traced.append(t/d);events=traces[round,case];invocations+=len(events);fractions.append([st.median(100*e['phase_ns'][i]/e['wall_ns']for e in events)for i in range(9)]);residual.append(st.median(100*(1-sum(e['phase_ns'])/e['wall_ns'])for e in events))
  summary[case]=dict(aa_absolute_percent=st.median(aa),disabled_over_original=st.median(disabled),traced_over_disabled=st.median(traced),trace_invocations=invocations,phase_percent={name:st.median(f[i]for f in fractions)for i,name in enumerate(NAMES)},unattributed_percent=st.median(residual))
 qualification=dict(diagnostic_complete=True,processes=48,wall_observations=384,observer_calibration_recorded=True,performance_qualified=False,full_objective_achieved=False);assert json.loads((raw/'summary.json').read_text())==summary and json.loads((raw/'qualification.json').read_text())==qualification
 return dict(audit_complete=True,**qualification,summary=summary)
def reversible_overlay(original,instrumented,patches):
 assert len(patches)==4 and [x['source']for x in patches]==['src/lib.rs','src/exec.rs','src/embed.rs','src/embed.rs']
 state=dict(original)
 for patch in patches:
  name=patch['source'];before=patch['before'].encode();after=patch['after'].encode();assert state[name].count(before)==1;state[name]=state[name].replace(before,after)
 assert state==instrumented,'unlisted source change'
 for patch in reversed(patches):
  name=patch['source'];before=patch['before'].encode();after=patch['after'].encode();assert state[name].count(after)==1;state[name]=state[name].replace(after,before)
 assert state==original
def audit(parent):
 raw=parent/'bulk-embedding-phases';run=json.loads((parent/'run-metadata.json').read_text());assert run.get('headSha',run.get('head_sha'))==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure'];frozen=git(INPUT+'/frozen-inputs.json');assert(raw/'frozen-inputs.json').read_bytes()==frozen
 for name,digest in json.loads(frozen).items():
  data=git(name);assert sha(data)==digest,name
  if name.startswith(INPUT+'/'):assert(raw/name[len(INPUT)+1:]).read_bytes()==data,name
 spec=json.loads((raw/'input.json').read_text());assert spec['profile_reference']=='a7d4b02c3'and spec['observer_only']and not spec['performance_qualified']
 for name,digest in spec['profile_source_sha256'].items():assert sha(git(name,spec['profile_reference']))==digest
 for name,digest in spec['checkout_source_sha256'].items():assert sha(git(name))==digest
 for name in ['exec','embed','lib']:assert(raw/f'original-{name}.rs').read_bytes()==git('src/'+name+'.rs',spec['profile_reference'])
 reversible_overlay({f'src/{name}.rs':(raw/f'original-{name}.rs').read_bytes()for name in ['exec','lib','embed']},{f'src/{name}.rs':(raw/f'instrumented-{name}.rs').read_bytes()for name in ['exec','lib','embed']},json.loads((raw/'patches.json').read_text()))
 original=(raw/'original-compare.rs').read_bytes();assert original==git('benches/results/2026-10-28-cow-cas-full-engine/compare.rs')
 expected=original.replace(b'fn main() -> airbug_bench::Result<()> {',b'fn main() -> airbug_bench::Result<()> {\n    lin::bulk_profile::configure();').replace(b'    run_suite(suite, &args)\n}',b'    let result = run_suite(suite, &args);\n    lin::bulk_profile::flush();\n    result\n}').replace(b'    ins.prepared.run(&mut ins.db).expect("lin insert").done.n',b'    lin::bulk_profile::start();\n    let count = ins.prepared.run(&mut ins.db).expect("lin insert").done.n;\n    lin::bulk_profile::finish();\n    count');assert(raw/'instrumented-compare.rs').read_bytes()==expected
 prov=json.loads((raw/'provenance.json').read_text());assert prov['all_sources_restored']and prov['profile_reference']==spec['profile_reference']and prov['restored_checkout_sha256']==spec['checkout_source_sha256']and prov['profile_source_sha256']==spec['profile_source_sha256']and not prov['performance_qualified']and prov['observer_only']
 for role in ['original','instrumented']:
  sources=dict(spec['profile_source_sha256'])
  for name in ['exec','lib','embed']:sources[f'src/{name}.rs']=sha((raw/f'{role}-{name}.rs').read_bytes())
  if role=='instrumented':sources['src/bulk_profile.rs']=sha((raw/'profile.rs').read_bytes())
  assert prov['binaries'][role]['source_sha256']==sources and prov['binaries'][role]['compare_sha256']==sha((raw/f'{role}-compare.rs').read_bytes())
 tests=json.loads((raw/'full-status.json').read_text());assert tests['exit']==0 and tests['all_sources_restored']and tests['compiled_sources_sha256']==prov['binaries']['instrumented']['source_sha256'];counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'full-tests.log').read_text());assert[sum(int(r[i])for r in counts)for i in range(3)]==[449,0,2]
 smokes=json.loads((raw/'smoke-status.json').read_text());assert len(smokes)==6 and {(s['role'],s['case'])for s in smokes}==set(itertools.product(['original','disabled','traced'],CASES))and all(s['exit']==0 and s['correctness_only']for s in smokes)
 assert 'FSTYPE' in(raw/'linux-environment.txt').read_text();env=json.loads((raw/'environment.json').read_text());assert 'linux'in env['platform'].lower()and 'x86_64'in env['platform']and 'release: 1.96.0'in env['rustc'];result=matrix(raw)
 return dict(**result,run_head=HEAD,profile_reference=spec['profile_reference'],instrumented_release_tests=449,ignored_tests=2,correctness_smokes=6,source_overlays_match_reversible_ledger=True)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('result_directory',type=Path);args=p.parse_args();q=audit(args.result_directory.resolve());(args.result_directory/'frozen-audit.json').write_text(json.dumps(q,indent=2)+'\n');(args.result_directory/'audited-report.md').write_bytes((args.result_directory/'bulk-embedding-phases/report.md').read_bytes());print(json.dumps(q,indent=2))
