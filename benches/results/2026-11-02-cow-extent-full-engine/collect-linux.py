"""Archive the complete new-protocol result without reclassifying earlier runs."""
from pathlib import Path
import hashlib,json,shutil,subprocess,tarfile,tempfile
p=Path(__file__).resolve().parent;repo=p.parents[2]
run='41401044023';expected='451fd7a25a6dc1f0f712eaef841aa8ee4bdd561d'
sha=lambda b:hashlib.sha256(b).hexdigest()
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','databaseId,status,conclusion,headSha,url,jobs'],cwd=repo,text=True))
assert meta['status']=='completed'and meta['headSha']==expected and meta['conclusion']in('success','failure')
steps=[s for j in meta['jobs']for s in j['steps']]
assert [s['name']for s in steps if s['conclusion']=='failure']in([],['Enforce unchanged full-source numerical thresholds'])
assert next(s for s in steps if s['name']=='Independent complete raw audit')['conclusion']=='success'
raw=Path('/tmp/lin-cow-extent-full-engine-'+run);assert not raw.exists(),'retain earlier download';raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','cow-extent-full-engine','--dir',str(raw)],cwd=repo,check=True)
frozen=json.loads((raw/'frozen.json').read_text())
for name,digest in frozen.items():
 assert sha(subprocess.check_output(['git','show',expected+':'+name],cwd=repo))==digest
 if name.startswith(str(p.relative_to(repo))+'/'):
  assert sha((raw/Path(name).relative_to(p.relative_to(repo))).read_bytes())==digest,name
for name in ['scripts/benchmark_role_balance.py']:
 assert sha((repo/name).read_bytes())==frozen[name],name
# Bind correctness logs and exact injected mutations independently of CI status flags.
import ast,re
inputs=json.loads((raw/'source-inputs.json').read_text());correct=json.loads((raw/'candidate-correctness.json').read_text())
assert correct['complete']and correct['sources_and_tests_restored']and correct['source_commit']==inputs['candidate']['commit']and correct['compiled_negative_checks']==5 and correct['compiled_negatives_exit_code']==0
assert 'release: 1.96.0'in correct['compiler']and'host: x86_64-unknown-linux-gnu'in correct['compiler']
for mode,passed,ignored in [('default',463,2),('feature',483,4)]:
 log=(raw/(mode+'-tests.log')).read_bytes();counts=[tuple(map(int,c))for c in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.decode())]
 assert dict(passed=sum(c[0]for c in counts),failed=sum(c[1]for c in counts),ignored=sum(c[2]for c in counts))==dict(passed=passed,failed=0,ignored=ignored)==correct['suites'][mode]['counts'];assert sha(log)==correct['suites'][mode]['log_sha256']and correct['suites'][mode]['exit_code']==0
source=subprocess.check_output(['git','show',inputs['candidate']['commit']+':src/exec.rs'],cwd=repo,text=True);tree=ast.parse((raw/'check-mutants.py').read_text());cases=next(ast.literal_eval(n.value)for n in tree.body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='cases'for t in n.targets));negative=json.loads((raw/'mutant-status.json').read_text());assert len(cases)==len(negative['checks'])==5 and negative['source_restored']
for (label,old,new,count,target,test),check in zip(cases,negative['checks']):
 assert source.count(old)==count;expected_mutant=source.replace(old,new).encode();assert(raw/(label+'-mutant.rs')).read_bytes()==expected_mutant
 assert check==dict(label=label,exit_code=101,test=test,mutant_sha256=sha(expected_mutant),compiled_runtime_rejected=True)
 log=(raw/(label+'-runtime.log')).read_text();assert 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and 'test '+test+' ... FAILED'in log
q=json.loads((raw/'qualification.json').read_text());audit=json.loads((raw/'independent-audit.json').read_text());provenance=json.loads((raw/'provenance.json').read_text())
assert q['measurement_complete']and q['processes']==2412 and q['observations']==19296
assert audit['audit_complete']and audit['invariant_peer_assignments']==1368 and audit['source_candidate_qualified']==q['source_candidate_qualified']
with tempfile.TemporaryDirectory(prefix='lin-shared-peer-recompute-')as td:
 copied=Path(td)/p.name;shutil.copytree(raw,copied)
 subprocess.run(['python3',str(copied/'summarize.py')],cwd=repo,check=True)
 subprocess.run(['python3',str(copied/'audit-baseline.py')],cwd=repo,check=True)
 assert json.loads((raw/'correctness-prerequisites.json').read_text())==json.loads((copied/'correctness-prerequisites.json').read_text())
 for name in ['summary.json','qualification.json','peer-comparisons.json']:
  assert json.loads((raw/name).read_text())==json.loads((copied/name).read_text())
binaries=Path('/tmp/lin-cow-extent-full-engine-executables-'+run);assert not binaries.exists(),'retain earlier executables';binaries.mkdir()
subprocess.run(['gh','run','download',run,'--name','cow-extent-full-engine-exact-linux-executables','--dir',str(binaries)],cwd=repo,check=True)
roles=dict(provenance['binaries'],shared_peer=provenance['peer_binary']);verified={}
for role,info in roles.items():
 f=binaries/Path(info['path']).name;b=f.read_bytes()
 assert sha(b)==info['sha256']and b[:6]==b'\x7fELF\x02\x01'and int.from_bytes(b[18:20],'little')==62
 verified[role]=dict(path=str(f),bytes=len(b),sha256=sha(b),elf64_x86_64=True)
assert verified['baseline']['sha256']==verified['shared_peer']['sha256']!=verified['candidate']['sha256']
# Independently bind every recorded assignment to the frozen dispatch rule.
dispatch={};exec(compile((raw/'assignment.py').read_text(),str(raw/'assignment.py'),'exec'),dispatch)
records=json.loads((raw/'process-status.json').read_text());external=0
for r in records:
 role,info=dispatch['binary_for'](provenance,r['mode'],r['case'])
 assert r['binary_role']==role and r['binary_sha256']==info['sha256']and r['command'][5]==info['path']and r['exit_code']==0
 external+=role=='shared_peer'
assert len(records)==2412 and external==1368
out=repo/'benches/results/2026-11-02-cow-extent-full-engine-linux';out.mkdir(exist_ok=False)
files=sorted(f for f in raw.rglob('*')if f.is_file())
manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tar:
 for f in files:tar.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(archive)as tar:
 assert len(tar.getmembers())==len(manifest)
 for member in tar:
  b=tar.extractfile(member).read();assert len(b)==manifest[member.name]['bytes']and sha(b)==manifest[member.name]['sha256']
for name in ['summary.json','qualification.json','peer-comparisons.json','independent-audit.json','report.md']:shutil.copy2(raw/name,out/name)
summary=json.loads((out/'summary.json').read_text());failed=[dict(case=c,series=label,**r)for c,rows in summary.items()for label,r in rows.items()if not r['passes']]
values={'raw-manifest.json':manifest,'run-metadata.json':meta,'failed-windows.json':failed,'decision.json':dict(source_gate_passes=q['source_candidate_qualified'],protocol='invariant external executable',earlier_failures_reclassified=False,adopted=False,full_objective_achieved=False,unchanged_retry=False),'binary-artifact.json':dict(run_id=int(run),artifact='cow-extent-full-engine-exact-linux-executables',verified_local_files=verified,binary_payloads_in_git=False),'archive-audit.json':dict(files=len(files),all_member_hashes_verified=True,numerical_recompute_identical=True,exact_executable_hashes_verified=True,invariant_peer_assignments_verified=external,archive_sha256=sha(archive.read_bytes()),runtime_adopted=False,full_objective_achieved=False)}
for name,value in values.items():(out/name).write_text(json.dumps(value,indent=2)+'\n')
print(json.dumps(dict(archive=str(out),files=len(files),source_gate_passes=q['source_candidate_qualified'],invariant_peer_assignments=external)))
