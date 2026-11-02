from pathlib import Path
import hashlib,json,math,os,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sys.path.insert(0,str(root/'scripts'));from benchmark_source_integrity import snapshot
sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text());build=json.loads((p/'build-provenance.json').read_text())
private=Path(os.environ['RUNNER_TEMP'])/'lin-cow-extent-mssql-private';assert not private.exists();private.mkdir(mode=0o700)
# Nothing containing relative performance is printed or written into the repo.
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
qa=json.loads((p/'correctness-prerequisite.json').read_text());assert qa['exact_linux_correctness_verified']and qa['default_passed']==463 and qa['feature_passed']==483 and qa['compiled_negative_checks']==5
assert build['features']==m['features']==['default','experimental-extent-wal']and build['source_unchanged']
assert build['source_sha256']==snapshot(root,None)['files_sha256'];binary=Path(build['binary']['path']);assert sha(binary.read_bytes())==build['binary']['sha256']
command=[sys.executable,'scripts/bench-mssql-native-ci.py','--output',str(private/'measurements'),'--binary',str(binary),'--image',m['image']]
with(private/'command.json').open('w')as f:json.dump(command,f)
result=subprocess.run(command,cwd=root);assert result.returncode==0,'Private functional qualification incomplete'
data=private/'measurements';env=json.loads((data/'environment.json').read_text());assert env['status']=='finished'and env['source_unchanged']and env['cleanup']==dict(stop_exit=0,remove_exit=0)
assert env['image']['Architecture']=='amd64'and env['image']['Os']=='linux'and m['image']in env['image']['RepoDigests']
assert env['binary_sha256']==build['binary']['sha256'];assert len(env['runs'])==6
samples=0
for mode in ['read','native']:
 cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']if mode=='read'else['insert_native']
 for rows in [1000,10000,100000]:
  d=json.loads((data/mode/f'rows-{rows}'/'run.json').read_text());assert d['status']=='complete'and not d['errors']and not d['source_drift']and len(d['engines'])==2 and set(d['engines'])=={'lin','mssql'}and d['cases']==cases and d['rows']==rows and d['samples']==12 and d['repeats']==3
  assert d['source_identity_before']==d['source_identity_after']
  identity=d['source_identity_before'];assert identity['files_sha256']==build['source_sha256']and identity['binary']['sha256']==build['binary']['sha256']
  assert len(d['observations'])==len(cases)*2*3
  assert {(o['case'],o['repetition'])for o in d['observations']}=={(c+'/'+engine,r)for c in cases for engine in ['lin','mssql']for r in range(3)}
  for o in d['observations']:
   values=o['samples_ns'];assert len(values)==12 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in values);samples+=len(values)
assert samples==1512
assert snapshot(root,None)['files_sha256']==build['source_sha256']and sha(binary.read_bytes())==build['binary']['sha256']
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
(private/'correctness-prerequisite.json').write_bytes((p/'correctness-prerequisite.json').read_bytes())
(private/'build-provenance.json').write_bytes((p/'build-provenance.json').read_bytes());(private/'frozen-input.json').write_bytes((p/'input.json').read_bytes())
(private/'functional-audit.json').write_text(json.dumps(dict(complete=True,reports=6,samples_validated=samples,source_unchanged=True,binary_verified=True,cleanup_verified=True,numerical_outcomes_public=False),indent=2)+'\n')
print('Current-source functional qualification and source audit completed; results remain private')
