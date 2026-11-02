"""Retain and independently recompute the complete changed-host API matrix."""
from pathlib import Path
import hashlib,json,math,shutil,statistics as st,subprocess,tarfile,sys,argparse
p=Path(__file__).resolve().parent;repo=p.parents[2]
parser=argparse.ArgumentParser();parser.add_argument('--run',required=True);parser.add_argument('--head',required=True);parser.add_argument('--resume',action='store_true');args=parser.parse_args();run=args.run;expected=args.head;assert run.isdecimal() and len(expected)==40;sha=lambda b:hashlib.sha256(b).hexdigest()
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','databaseId,headSha,status,conclusion,url,jobs'],cwd=repo,text=True))
assert meta['status']=='completed'and meta['headSha']==expected and meta['conclusion']in('success','failure')
steps=[s for j in meta['jobs']for s in j['steps']]
assert [s['name']for s in steps if s['conclusion']=='failure']in([],['Enforce unchanged available-peer process win gate'])
for name in ['Audit all observations values and original processes','Verify owned containers absent','Verify final frozen inputs without overlay or source mutation']:
 assert next(s for s in steps if s['name']==name)['conclusion']=='success'
resume=args.resume
raw=Path('/tmp/lin-cow-extent-public-peer-'+run)
if raw.exists():assert resume,'retain earlier download; explicit --resume revalidates all files'
else:
 assert not resume,'no earlier download to resume'
 raw.mkdir();subprocess.run(['gh','run','download',run,'--name','cow-extent-public-peers','--dir',str(raw)],cwd=repo,check=True)
load=lambda name:json.loads((raw/name).read_text())
inputs=load('input.json');assert(raw/'input.json').read_bytes()==subprocess.check_output(['git','show',expected+':'+str(p.relative_to(repo)/'input.json')],cwd=repo)
for name,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',expected+':'+name],cwd=repo))==h
 if name.startswith(str(p.relative_to(repo))+'/'):
  assert sha((raw/Path(name).relative_to(p.relative_to(repo))).read_bytes())==h,name
env=load('environment.json');build=load('build-provenance.json');audit=load('audit.json');independent=load('independent-process-audit.json')
assert env['status']=='finished'and env['source_unchanged']and build['source_unchanged']
assert env['runtime_reference']==build['runtime_reference']==inputs['runtime_reference']=='60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'
assert env['source_commit']==build['source_commit']==expected and env['source_sha256']==build['source_sha256']
assert {n:h for n,h in env['source_sha256'].items()if n.startswith('src/')}==inputs['runtime_source_sha256']
for name,h in env['source_sha256'].items():
 ref=inputs['runtime_reference']if name.startswith('src/')else expected
 assert sha(subprocess.check_output(['git','show',ref+':'+name],cwd=repo))==h,name
assert 'release: 1.96.0'in env['rustc']and 'host: x86_64-unknown-linux-gnu'in env['rustc']
assert build['features']==inputs['features']==['default','experimental-extent-wal']
assert build['build_command']==['cargo','build','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics']
prerequisite=load('correctness-prerequisite.json');assert prerequisite['exact_linux_correctness_verified']and prerequisite['default_passed']==463 and prerequisite['feature_passed']==483 and prerequisite['compiled_negative_checks']==5
assert prerequisite['upstream']['databaseId']==41401044023 and prerequisite['upstream']['headSha']=='451fd7a25a6dc1f0f712eaef841aa8ee4bdd561d'
for name,h in prerequisite['artifact_sha256'].items():assert sha((raw/'correctness-artifacts'/name).read_bytes())==h,name
assert len(env['cleanup'])==3 and all(v==dict(stop_exit=0,remove_exit=0)for v in env['cleanup'].values())
assert load('cleanup-proof.json')['all_owned_containers_absent']and env['host_guard']['kernel_guard_passed']
assert set(env['images'])==set(inputs['images'].values())
assert all(v['Architecture']=='amd64'and v['Os']=='linux'and ref in v['RepoDigests']for ref,v in env['images'].items())
assert independent['audit_complete']and independent['original_processes']==18 and independent['validated_samples']==5184
def verify_identity(record):
 assert record['source_identity_before']==record['source_identity_after']
 identity=record['source_identity_before'];assert identity['binary']['sha256']==env['binary_sha256']
 assert {n:h for n,h in identity['files_sha256'].items()if n.startswith('src/')}==inputs['runtime_source_sha256']
 assert identity['files_sha256']==env['source_sha256'],'Complete worker/derive/build closure differs'

summary={};results=[];versions={};observations=0;processes=0
for mode in ['read','native']:
 cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']if mode=='read'else['insert_native']
 engines={'lin','sqlite','duckdb','postgres','mysql','mongo'}|({'pandas'}if mode=='read'else set())
 comparisons=[];sample_count=0
 for n in [1000,10000,100000]:
  d=load(f'{mode}/rows-{n}/run.json');verify_identity(d);versions.update(d['versions'])
  assert d['status']=='complete'and not d['errors']and not d['source_drift']and d['rows']==n and d['samples']==12 and d['repeats']==3
  assert d['git_commit']==expected and d['lin_binary_info']['sha256']==env['binary_sha256']and d['cases']==cases and set(d['engines'])==engines
  assert len(d['observations'])==len(cases)*len(engines)*3
  assert {(o['case'],o['repetition'])for o in d['observations']}=={(c+'/'+e,i)for c in cases for e in engines for i in range(3)}
  for i in range(3):
   child=load(f'{mode}/rows-{n}/process-{i+1}/run.json');verify_identity(child);processes+=1
   assert child['status']=='complete'and not child['errors']and not child['source_drift']
   assert child['rows']==n and child['samples']==12 and child['repeats']==1 and child['git_commit']==expected
   assert child['lin_binary_info']['sha256']==env['binary_sha256']and child['cases']==cases and set(child['engines'])==engines
   assert len(child['observations'])==len(cases)*len(engines)
   for case in cases:
    expected_rows={'point_get':1,'filter_eq':1,'text_substr':1,'materialize':n//2,'join_inner':n,'join_filter':sum(k%200>100 for k in range(n)),'insert_native':n}[case]
    for engine in engines:
     key=case+'/'+engine;o=next(o for o in d['observations']if o['case']==key and o['repetition']==i)
     original=next(o for o in child['observations']if o['case']==key)
     assert {**original,'repetition':i}==o and o['validated_rows']==expected_rows and o['iterations']>0 and len(o['samples_ns'])==12
     assert all(type(v)in [int,float]and math.isfinite(v)and v>0 for v in o['samples_ns'])
     assert child['medians_ns'][key]==st.median(o['samples_ns']);sample_count+=12;observations+=12
  for case in cases:
   for engine in engines:
    key=case+'/'+engine;assert d['medians_ns'][key]==st.median(st.median(o['samples_ns'])for o in d['observations']if o['case']==key)
   for peer in sorted(engines-{'lin'}):
    ratios=[]
    for i in range(3):
     vals={e:st.median(next(o for o in d['observations']if o['case']==case+'/'+e and o['repetition']==i)['samples_ns'])for e in ['lin',peer]};ratios.append(vals[peer]/vals['lin'])
    lin=d['medians_ns'][case+'/lin'];other=d['medians_ns'][case+'/'+peer]
    comparisons.append(dict(rows=n,case=case,peer=peer,lin_ns=lin,peer_ns=other,speedup=other/lin,process_speedups=ratios,positive_processes=sum(v>1 for v in ratios)))
  check=next(v for v in load(mode+'/verification.json')['runs']if v['rows']==n)
  assert check['sentinels_unchanged']and all(not v for v in check['fixture_objects_remaining'].values())
 summary[mode]=dict(comparisons=len(comparisons),wins=sum(v['speedup']>1 for v in comparisons),samples=sample_count,process_comparisons=len(comparisons)*3,positive_processes=sum(v['positive_processes']for v in comparisons),minimum_speedup=min(v['speedup']for v in comparisons))
 results.append({'mode':mode,**summary[mode],'comparisons':comparisons})
assert observations==5184 and processes==18 and summary['read']['comparisons']==108 and summary['native']['comparisons']==15
assert audit==dict(runtime_reference=env['runtime_reference'],source_commit=expected,versions=versions,summary=summary,results=results)
binaries=Path('/tmp/lin-cow-extent-public-peer-executable-'+run)
if binaries.exists():assert resume,'preserve earlier executable download'
else:
 binaries.mkdir();subprocess.run(['gh','run','download',run,'--name','cow-extent-public-peers-exact-linux-executable','--dir',str(binaries)],cwd=repo,check=True)
f=binaries/Path(build['binary']['path']).name;b=f.read_bytes()
assert sha(b)==build['binary']['sha256']==env['binary_sha256']and b[:6]==b'\x7fELF\x02\x01'and int.from_bytes(b[18:20],'little')==62
out=repo/'benches/results/2026-11-02-cow-extent-public-peers-linux';out.mkdir(exist_ok=False)
files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tar:
 for f in files:tar.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tar:
 assert len(tar.getmembers())==len(manifest)
 for member in tar:
  data=tar.extractfile(member).read();assert len(data)==manifest[member.name]['bytes']and sha(data)==manifest[member.name]['sha256']
for name in ['audit.json','independent-process-audit.json','report.md','environment.json','correctness-prerequisite.json']:shutil.copy2(raw/name,out/name)
values={'raw-manifest.json':manifest,'run-metadata.json':meta,'archive-audit.json':dict(files=len(files),all_member_hashes_verified=True,frozen_git_blobs_verified=True,numerical_recompute_identical=True,original_processes=processes,validated_samples=observations,exact_linux_executable_verified=True,archive_sha256=sha(archive.read_bytes()),performance_adopted=False,full_objective_achieved=False),'binary-artifact.json':dict(path=str(f),sha256=sha(b),bytes=len(b),elf64_x86_64=True,binary_payloads_in_git=False),'decision.json':dict(available_peer_gate_passes=all(v['wins']==v['comparisons']and v['positive_processes']==v['process_comparisons']for v in summary.values()),upstream_source_gate_passes=prerequisite['upstream_source_gate_passed'],adopted=False,full_objective_achieved=False,unchanged_retry=False)}
# The executable path must refer to the verified binary, not a raw archive member.
values['binary-artifact.json']['path']=str(binaries/Path(build['binary']['path']).name)
for name,value in values.items():(out/name).write_text(json.dumps(value,indent=2)+'\n')
print(json.dumps(dict(archive=str(out),files=len(files),source_restored=True,processes=processes,samples=observations,summary=summary)))
