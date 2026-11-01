#!/usr/bin/env python3
"""Bind current full-value peer aggregates to each original process and frozen source."""
from pathlib import Path
import hashlib,json,math,statistics,subprocess,sys
root=Path(__file__).resolve().parents[1]
p=Path(sys.argv[1]).resolve()if len(sys.argv)>1 else root/'benches/results/2026-11-01-corrected-cow-peer-refresh-local'
load=lambda f:json.loads(f.read_text())
sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
env=load(p/'environment.json');inputs=load(p/'input.json');old=load(p/'audit.json')
assert env['status']=='finished'and env['source_unchanged']
for name,digest in inputs['files_sha256'].items():assert sha(root/name)==digest,name
for name,digest in env['source_sha256'].items():assert sha(root/name)==digest,name
expected_source=load(root/'benches/results/2026-11-01-corrected-cow-full-engine/source-inputs.json')['candidate']['source_sha256']
assert {n:h for n,h in env['source_sha256'].items()if n.startswith('src/')}==expected_source
assert sha(Path('/tmp/lin-cow-corrected-peer-current'))==env['binary_sha256']
assert len(env['cleanup'])==3 and all(r==dict(stop_exit=0,remove_exit=0)for r in env['cleanup'].values())
assert load(p/'cleanup-proof.json')['all_owned_containers_absent']
assert len(env['images'])==3 and all(v['Architecture']=='arm64'and v['Os']=='linux'for v in env['images'].values())
count=0;processes=0
for mode in ['read','native']:
 cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']if mode=='read'else['insert_native']
 engines={'lin','sqlite','duckdb','postgres','mysql','mongo'}|({'pandas'}if mode=='read'else set())
 for n in [1000,10000,100000]:
  directory=p/mode/f'rows-{n}';d=load(directory/'run.json')
  assert d['status']=='complete'and not d['errors']and not d['source_drift']
  assert d['rows']==n and d['samples']==12 and d['repeats']==3 and d['cases']==cases and set(d['engines'])==engines
  assert d['git_commit']==env['source_commit']and d['lin_binary_info']['sha256']==env['binary_sha256']
  assert len(d['observations'])==len(cases)*len(engines)*3
  assert {(o['case'],o['repetition'])for o in d['observations']}=={(c+'/'+e,i)for c in cases for e in engines for i in range(3)}
  for i in range(3):
   child=load(directory/f'process-{i+1}'/'run.json');processes+=1
   assert child['status']=='complete'and not child['errors']and not child['source_drift']
   assert child['rows']==n and child['samples']==12 and child['repeats']==1
   assert child['cases']==cases and set(child['engines'])==engines
   assert child['git_commit']==env['source_commit']and child['lin_binary_info']['sha256']==env['binary_sha256']
   assert len(child['observations'])==len(cases)*len(engines)
   for c in cases:
    expected_rows={'point_get':1,'filter_eq':1,'text_substr':1,'materialize':n//2,'join_inner':n,'join_filter':sum(k%200>100 for k in range(n)),'insert_native':n}[c]
    for e in engines:
     key=c+'/'+e;o=next(o for o in d['observations']if o['case']==key and o['repetition']==i)
     original=next(o for o in child['observations']if o['case']==key)
     assert {**original,'repetition':i}==o
     assert o['validated_rows']==expected_rows and o['iterations']>0 and len(o['samples_ns'])==12
     assert all(type(x)in [int,float]and math.isfinite(x)and x>0 for x in o['samples_ns']);count+=12
     assert child['medians_ns'][key]==statistics.median(o['samples_ns'])
  for c in cases:
   for e in engines:
    key=c+'/'+e;assert d['medians_ns'][key]==statistics.median(statistics.median(o['samples_ns'])for o in d['observations']if o['case']==key)
  verification=next(r for r in load(p/mode/'verification.json')['runs']if r['rows']==n)
  assert verification['sentinels_unchanged']and all(not x for x in verification['fixture_objects_remaining'].values())
assert count==5184 and processes==18
assert old['source_commit']==env['source_commit']
result=dict(audit_complete=True,source_commit=env['source_commit'],runtime_reference=inputs['runtime_reference'],original_processes=18,validated_samples=5184,full_objective_achieved=False,scope='Local ARM prepared Rust versus Python/driver/network API comparison; not Full durable source acceptance',auditor_sha256=sha(Path(__file__).resolve()))
(p/'independent-process-audit.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS: 18 original processes /5184 samples, full values and frozen sources verified')
